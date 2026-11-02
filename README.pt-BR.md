<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/logo-dark.svg">
    <img src="assets/branding/logo-light.svg" alt="Rigelcam" height="80">
  </picture>
</p>

<p align="center"><a href="README.md">English</a> · Português · <a href="README.es.md">Español</a></p>

O Rigelcam é um app de desktop para ver câmeras ao vivo e navegar pelas gravações de NVRs Intelbras
e Dahua, no Linux e no Windows.

Ele conversa com o gravador pelos protocolos que o próprio aparelho já oferece, RTSP para o vídeo e a
API HTTP da Dahua para o resto. Por isso não precisa do SDK do fabricante e roda nativo no Linux.

> [!NOTE]
> Fase inicial. O app está em uso diário no Linux. No Windows, ao vivo, gravações, exportação e fotos
> foram testados com vídeo Intel, e as opções RTX numa GeForce RTX 4070.

## Recursos

**Ao vivo**

- Grade de câmeras de um ou mais NVRs, usando o substream para a grade cheia continuar leve
- Clique duplo para abrir uma câmera sozinha no stream principal, com zoom digital e gamma
- RTX Super Resolution e RTX HDR no Windows com placa NVIDIA RTX, desligados por padrão
- Foto salva na resolução do vídeo
- Nomes dos canais lidos do gravador
- Câmeras sem permissão para o usuário aparecem assim, sem novas tentativas

**Gravações**

- Linha do tempo por canal em qualquer dia, com os eventos de movimento separados da gravação contínua
- Calendário com os dias que têm gravação
- Clique em qualquer ponto da linha do tempo para reproduzir a partir dali, em até 8x
- Até 4 canais reproduzidos em sincronia, cada um com seu zoom digital
- Exportação de trechos em MP4, sem recompressão, só das gravações que o usuário pode reproduzir

A interface está em inglês, português e espanhol, com tema escuro e claro.

## Como funciona

| O quê | Como |
| --- | --- |
| Vídeo ao vivo | RTSP, `/cam/realmonitor?channel=N&subtype=0\|1` |
| Lista de gravações | Sessão da API web (`RPC2`), ou HTTP Digest com `mediaFileFind.cgi` |
| Reprodução | RTSP, `/cam/playback?channel=N&starttime=...&endtime=...`, ou `loadfile.cgi` nos canais que recusam |
| Exportação | HTTP Digest, `loadfile.cgi` |
| Dados do aparelho e dos canais | HTTP Digest, `magicBox.cgi` e `configManager.cgi` |

O vídeo é decodificado pelo GStreamer, com aceleração por hardware quando disponível (VA-API no
Linux, Direct3D 11 no Windows). O núcleo é escrito em Rust e a interface em Qt 6 Quick.

Alguns gravadores respondem 404 por RTSP para câmeras com firmware recente, que continuam gravando
normalmente. Nesses canais, a gravação toca por HTTP e o vídeo ao vivo abre direto na câmera, no
endereço informado pelo gravador e com o login dele.

Testado até agora com os NVRs Intelbras NVD 1308 e NVD 7132. Outros gravadores com base Dahua devem
funcionar do mesmo jeito, e relatos são bem-vindos.

Acesso remoto pela nuvem da Intelbras (P2P) está fora do escopo. Para acessar o gravador de fora da
rede dele, use uma VPN.

## Credenciais

As senhas dos gravadores ficam no cofre de senhas do sistema (Secret Service no Linux, Gerenciador
de Credenciais no Windows) e nunca são gravadas em arquivos de configuração nem em logs.

No Windows, o app consulta a última versão no GitHub ao abrir e mostra um aviso se houver uma nova.
Nada é baixado sem você pedir, e um instalador só roda se a assinatura for válida. Para desligar a
consulta, use `"check_updates": false` em `recorders.json`.

O tráfego de vídeo e HTTP não é criptografado, então mantenha os gravadores numa rede confiável. A
biblioteca de RTSP responde a um servidor que peça autenticação Basic, que envia a senha de forma
reversível. Os gravadores usam Digest, e o risco é um aparelho na rede se passar por um deles.

## Instalando

Baixe na [página de versões](https://github.com/eduardoghi/rigelcam/releases). No Windows, o
instalador atualiza o app sozinho. No Linux, instale o pacote Flatpak:

```sh
flatpak install --user rigelcam-*-linux-x86_64.flatpak
```

O pacote busca o runtime no Flathub e não se atualiza sozinho, então instale cada versão nova do
mesmo jeito. Os dados dele ficam separados de uma compilação feita do código, então cadastre os
gravadores de novo.

## Compilando

É preciso de Rust 1.96 ou mais novo, Qt 6 com Qt Quick e as ferramentas do Linguist, e GStreamer
1.24 ou mais novo com os plugins base, good e bad.

```sh
cargo build --release -p rigelcam-app
```

O app fica em `target/release`. No Linux, a decodificação por hardware precisa de VA-API. No
Windows, compile num ambiente do Visual Studio, com `QMAKE` apontando para o `qmake` de um Qt para
MSVC, `PKG_CONFIG` para o `pkg-config` do GStreamer oficial para MSVC e as pastas `bin` dos dois no
`PATH`.

## Contribuindo

Ative a validação da mensagem de commit e da formatação uma vez por clone:

```sh
git config core.hooksPath .githooks
```

Relate problemas de segurança como descrito em [SECURITY.md](SECURITY.md).

## Licença

[MIT](LICENSE). A fonte Space Grotesk usada no logo está sob a
[SIL Open Font License](assets/branding/SpaceGrotesk-OFL.txt).

O app usa Qt e GStreamer, disponíveis sob a LGPL. Uma versão distribuída precisa cumprir essas
licenças, e alguns plugins do GStreamer têm termos próprios.

## Aviso

O Rigelcam é um projeto independente, sem vínculo com a Intelbras ou a Dahua e sem endosso delas.
Intelbras e Dahua são marcas de seus respectivos donos.
