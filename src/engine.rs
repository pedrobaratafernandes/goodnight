//! Background worker: counts down, fades the volume and finally runs the action.
//! Runs on its own thread so it keeps working while the window is minimised.

use crate::config::{Action, Mode, Options, TimerSettings};
use crate::curve::Profile;
use crate::system;
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Fade {
    pub duration: Duration,
    pub profile: Profile,
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub deadline: DateTime<Local>,
    pub action: Action,
    pub fade: Option<Fade>,
    pub force: bool,
    pub restore: Option<f32>,
}

impl Plan {
    /// When the fade begins (never before `now`; compressed if there isn't enough time).
    pub fn fade_start(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        let f = self.fade.as_ref()?;
        let ideal = self.deadline - ChronoDuration::from_std(f.duration).ok()?;
        Some(ideal.max(now).min(self.deadline))
    }
}

/// Next occurrence of `hour:minute` strictly after `now` (today or tomorrow).
pub fn next_occurrence(now: DateTime<Local>, hour: u32, minute: u32) -> DateTime<Local> {
    let time = NaiveTime::from_hms_opt(hour.min(23), minute.min(59), 0).unwrap();
    let mut date = now.date_naive();
    loop {
        // `earliest` handles DST gaps/overlaps; a missing local time is skipped.
        if let Some(dt) = Local.from_local_datetime(&date.and_time(time)).earliest()
            && dt > now
        {
            return dt;
        }
        date = date.succ_opt().unwrap();
    }
}

pub fn deadline_for(s: &TimerSettings, now: DateTime<Local>) -> DateTime<Local> {
    match s.mode {
        Mode::Countdown => now + ChronoDuration::from_std(s.unit.to_duration(s.amount)).unwrap(),
        Mode::AtTime => next_occurrence(now, s.at_hour, s.at_minute),
    }
}

pub fn make_plan(s: &TimerSettings, o: &Options, now: DateTime<Local>) -> Plan {
    Plan {
        deadline: deadline_for(s, now),
        action: s.action,
        fade: s.fade_enabled.then(|| Fade {
            duration: s.fade_unit.to_duration(s.fade_amount),
            profile: s.profile,
            start: s.start_volume as f32 / 100.0,
            end: s.end_volume as f32 / 100.0,
        }),
        force: o.force_quit,
        restore: (o.restore_enabled && s.action.is_power_action())
            .then(|| o.restore_volume as f32 / 100.0),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Waiting,
    Fading,
    Executing,
    Done,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Status {
    pub phase: Phase,
    pub plan: Plan,
    pub fade_start: Option<DateTime<Local>>,
    pub volume: Option<f32>,
    pub warning: Option<String>,
}

pub struct Engine {
    status: Arc<Mutex<Status>>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start(plan: Plan) -> Self {
        let now = Local::now();
        let status = Arc::new(Mutex::new(Status {
            phase: Phase::Waiting,
            fade_start: plan.fade_start(now),
            plan: plan.clone(),
            volume: None,
            warning: None,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let handle = {
            let status = status.clone();
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("goodnight-worker".into())
                .spawn(move || worker(plan, status, cancel))
                .expect("spawn worker")
        };
        Self {
            status,
            cancel,
            handle: Some(handle),
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status().phase,
            Phase::Waiting | Phase::Fading | Phase::Executing
        )
    }

    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

fn worker(plan: Plan, status: Arc<Mutex<Status>>, cancel: Arc<AtomicBool>) {
    system::init_thread();
    let set_status = |f: &dyn Fn(&mut Status)| f(&mut status.lock().unwrap());

    let initial = system::get_volume();
    set_status(&|s| match &initial {
        Ok(v) => s.volume = Some(*v),
        Err(e) => s.warning = Some(e.clone()),
    });

    let fade_start = plan.fade_start(Local::now());
    let mut last_set: Option<f32> = None;
    let mut fading = false;
    let mut ticks: u32 = 0;

    loop {
        if cancel.load(Ordering::SeqCst) {
            // If we already lowered the volume, give it back: the user is clearly awake.
            if fading && let Some(f) = &plan.fade {
                let _ = system::set_volume(f.start);
            }
            set_status(&|s| s.phase = Phase::Cancelled);
            return;
        }
        let now = Local::now();
        if now >= plan.deadline {
            break;
        }

        if let (Some(f), Some(fs)) = (&plan.fade, fade_start)
            && now >= fs
        {
            if !fading {
                fading = true;
                set_status(&|s| s.phase = Phase::Fading);
            }
            let total = (plan.deadline - fs).num_milliseconds().max(1) as f32;
            let t = (now - fs).num_milliseconds() as f32 / total;
            let v = f.profile.volume(f.start, f.end, t);
            if last_set.is_none_or(|l| (l - v).abs() >= 0.004) {
                match system::set_volume(v) {
                    Ok(()) => {
                        last_set = Some(v);
                        set_status(&|s| s.volume = Some(v));
                    }
                    Err(e) => set_status(&|s| s.warning = Some(e.clone())),
                }
            }
        } else if ticks.is_multiple_of(12) {
            // Not fading: refresh the displayed volume every ~3 s.
            if let Ok(v) = system::get_volume() {
                set_status(&|s| s.volume = Some(v));
            }
        }
        ticks = ticks.wrapping_add(1);
        std::thread::sleep(Duration::from_millis(250));
    }

    set_status(&|s| s.phase = Phase::Executing);
    if let Some(f) = &plan.fade {
        let _ = system::set_volume(f.end);
        set_status(&|s| s.volume = Some(f.end));
    }

    if plan.action == Action::CloseApp {
        set_status(&|s| s.phase = Phase::Done);
        std::thread::sleep(Duration::from_millis(300));
        std::process::exit(0);
    }

    match system::power_action(plan.action, plan.force) {
        Ok(()) => {
            // Put the sound back so the computer isn't silent next time it's used.
            if let Some(r) = plan.restore {
                let _ = system::set_volume(r);
                set_status(&|s| s.volume = Some(r));
            }
            set_status(&|s| s.phase = Phase::Done);
        }
        Err(e) => set_status(&|s| s.phase = Phase::Failed(e.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Unit;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 3, 10, h, m, 0).unwrap()
    }

    #[test]
    fn next_occurrence_today_or_tomorrow() {
        assert_eq!(next_occurrence(at(22, 0), 23, 30), at(23, 30));
        let tomorrow = next_occurrence(at(22, 0), 6, 15);
        assert_eq!(
            tomorrow.date_naive(),
            at(0, 0).date_naive().succ_opt().unwrap()
        );
        // same minute → tomorrow, never "now"
        assert!(next_occurrence(at(22, 0), 22, 0) > at(22, 0));
    }

    #[test]
    fn plan_and_fade_window() {
        let s = TimerSettings {
            mode: Mode::Countdown,
            amount: 30,
            unit: Unit::Minutes,
            fade_enabled: true,
            fade_amount: 10,
            fade_unit: Unit::Minutes,
            ..Default::default()
        };
        let now = at(22, 0);
        let p = make_plan(&s, &Options::default(), now);
        assert_eq!(p.deadline, at(22, 30));
        assert_eq!(p.fade_start(now), Some(at(22, 20)));

        // Fade longer than the remaining time → starts immediately.
        let s2 = TimerSettings {
            amount: 5,
            ..s.clone()
        };
        let p2 = make_plan(&s2, &Options::default(), now);
        assert_eq!(p2.fade_start(now), Some(now));

        // Restore only applies to power actions.
        let s3 = TimerSettings {
            action: Action::CloseApp,
            ..s
        };
        assert!(make_plan(&s3, &Options::default(), now).restore.is_none());
    }
}

/// End-to-end run against fake `wpctl`/`systemctl` scripts (Linux only):
/// `cargo test -- --ignored e2e`
#[cfg(all(test, target_os = "linux"))]
mod e2e {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    #[ignore]
    fn e2e_fade_then_poweroff() {
        let dir = std::env::temp_dir().join(format!("goodnight-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log.txt");
        let vol = dir.join("vol");
        std::fs::write(&vol, "0.70").unwrap();
        let wpctl = format!(
            "#!/bin/sh\nif [ \"$1\" = get-volume ]; then echo \"Volume: $(cat {v})\"; else echo \"$3\" > {v}; echo \"set $3\" >> {l}; fi\n",
            v = vol.display(),
            l = log.display()
        );
        let systemctl = format!("#!/bin/sh\necho \"systemctl $*\" >> {}\n", log.display());
        for (name, body) in [("wpctl", wpctl), ("systemctl", systemctl)] {
            let p = dir.join(name);
            std::fs::write(&p, body).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // SAFETY: single-threaded test setup before the worker starts.
        unsafe {
            std::env::set_var(
                "PATH",
                format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
            )
        };

        let now = Local::now();
        let plan = Plan {
            deadline: now + ChronoDuration::milliseconds(3000),
            action: Action::Shutdown,
            fade: Some(Fade {
                duration: Duration::from_secs(2),
                profile: Profile::Linear,
                start: 0.6,
                end: 0.1,
            }),
            force: true,
            restore: Some(0.5),
        };
        let engine = Engine::start(plan);
        std::thread::sleep(Duration::from_millis(3800));
        let st = engine.status();
        let out = std::fs::read_to_string(&log).unwrap();
        println!("{out}");
        assert_eq!(st.phase, Phase::Done);
        let sets: Vec<f32> = out
            .lines()
            .filter_map(|l| l.strip_prefix("set "))
            .map(|v| v.parse().unwrap())
            .collect();
        assert!(sets.len() > 5, "fade should set volume many times");
        assert!((sets[0] - 0.6).abs() < 0.05, "starts near start volume");
        assert!(
            sets.windows(2)
                .take(sets.len() - 2)
                .all(|w| w[1] <= w[0] + 1e-3),
            "decreasing during fade"
        );
        assert!(out.contains("systemctl poweroff --ignore-inhibitors"));
        assert!(
            out.trim_end().ends_with("set 0.500"),
            "volume restored after action"
        );
    }
}
