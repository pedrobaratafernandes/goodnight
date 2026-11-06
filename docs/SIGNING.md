# Assinatura do executável Windows (SignPath)

O workflow `.github/workflows/build.yml` já tem os passos de assinatura. Ficam desligados até existir a variável `SIGNPATH_ORGANIZATION_ID`.

## Depois de a candidatura ser aprovada

1. **No SignPath.io**, crie um projeto com:
   - **Slug:** `goodnight`
   - **Repository URL:** `https://github.com/pedrobaratafernandes/goodnight`
   - **Trusted build system:** GitHub.com
2. **Configuração do artefacto.** O GitHub entrega o executável dentro de um zip. Use:
   ```xml
   <?xml version="1.0" encoding="utf-8"?>
   <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
     <zip-file>
       <pe-file path="goodnight.exe" product-name="Goodnight" product-version="*">
         <authenticode-sign />
       </pe-file>
     </zip-file>
   </artifact-configuration>
   ```
   O `build.rs` já grava `ProductName = Goodnight` e a versão do `Cargo.toml` no `.exe`, como a SignPath Foundation exige.
3. **Políticas de assinatura:** `test-signing` (para builds normais) e `release-signing` (para tags `v*`, com aprovação manual).
4. **No GitHub**, em *Settings → Secrets and variables → Actions*:
   - **Secret** `SIGNPATH_API_TOKEN`: o token de API de um utilizador com o papel *Submitter*.
   - **Variable** `SIGNPATH_ORGANIZATION_ID`: o ID da organização (aparece no canto superior direito do SignPath).
5. Lance uma nova versão (`git tag -a v1.0.1 -m "..." && git push --follow-tags`), aprove o pedido no SignPath e a Release sai com o `.exe` assinado.

## Requisitos da SignPath Foundation a manter

- Autenticação de dois fatores (MFA) ativa no GitHub e no SignPath.
- Secção "Code signing policy" no README e nas páginas de release (já incluída).
- Aprovação manual de cada release.
