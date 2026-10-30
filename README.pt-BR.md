<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/logo-dark.svg">
    <img src="assets/branding/logo-light.svg" alt="rigelcam" height="80">
  </picture>
</p>

<p align="center"><a href="README.md">English</a> · Português</p>

O rigelcam é um app de desktop para ver câmeras ao vivo e navegar pelas gravações de NVRs Intelbras
e Dahua, no Linux e no Windows.

Ele conversa com o gravador pelos protocolos que o próprio aparelho já oferece, RTSP para o vídeo e a
API HTTP da Dahua para o resto. Por isso não precisa do SDK do fabricante e roda nativo no Linux.

> [!NOTE]
> Fase inicial. Ainda não existe versão para usar. A parte de protocolo descrita abaixo já foi
> confirmada em aparelhos reais, e o app em volta dela está sendo escrito.

## Recursos planejados

**Ao vivo**

- Grade de câmeras de um ou mais NVRs, usando o substream para a grade cheia continuar leve
- Clique duplo para abrir uma câmera sozinha no stream principal
- Nomes dos canais lidos do gravador

**Gravações**

- Linha do tempo por canal em qualquer dia, com os eventos de movimento separados da gravação contínua
- Clique em qualquer ponto da linha do tempo para reproduzir a partir dali
- Vários canais reproduzidos em sincronia
- Avanço rápido, se o gravador aceitar (ainda em teste)
- Exportação de trechos em MP4

## Como funciona

| O quê | Como |
| --- | --- |
| Vídeo ao vivo | RTSP, `/cam/realmonitor?channel=N&subtype=0\|1` |
| Lista de gravações | HTTP Digest, `mediaFileFind.cgi` |
| Reprodução | RTSP, `/cam/playback?channel=N&starttime=...&endtime=...` |
| Dados do aparelho e dos canais | HTTP Digest, `magicBox.cgi` e `configManager.cgi` |

O vídeo é decodificado pelo GStreamer, com aceleração por hardware quando disponível (VA-API no
Linux, Direct3D 11 no Windows). O núcleo é escrito em Rust e a interface em Qt 6 Quick.

Testado até agora com os NVRs Intelbras NVD 1308 e NVD 7132. Outros gravadores com base Dahua devem
funcionar do mesmo jeito, e relatos são bem-vindos.

Acesso remoto pela nuvem da Intelbras (P2P) está fora do escopo. Para acessar o gravador de fora da
rede dele, use uma VPN.

## Credenciais

As senhas dos gravadores ficam no cofre de senhas do sistema (Secret Service no Linux, Gerenciador
de Credenciais no Windows) e nunca são gravadas em arquivos de configuração nem em logs.

## Contribuindo

Ative a validação da mensagem de commit e da formatação uma vez por clone:

```sh
git config core.hooksPath .githooks
```

## Licença

[MIT](LICENSE). A fonte Space Grotesk usada no logo está sob a
[SIL Open Font License](assets/branding/SpaceGrotesk-OFL.txt).

## Aviso

O rigelcam é um projeto independente, sem vínculo com a Intelbras ou a Dahua e sem endosso delas.
Intelbras e Dahua são marcas de seus respectivos donos.
