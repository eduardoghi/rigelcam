<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/logo-dark.svg">
    <img src="assets/branding/logo-light.svg" alt="Rigelcam" height="80">
  </picture>
</p>

<p align="center"><a href="README.md">English</a> · <a href="README.pt-BR.md">Português</a> · Español</p>

Rigelcam es una aplicación de escritorio para ver cámaras en vivo y consultar grabaciones en NVR
Intelbras y Dahua, para Linux y Windows.

Se comunica con el grabador por los protocolos que el equipo ya ofrece, RTSP para el video y la API
HTTP de Dahua para todo lo demás, así que no necesita el SDK del fabricante y funciona de forma
nativa en Linux.

> [!NOTE]
> Etapa inicial, todavía sin versión empaquetada. La aplicación se usa a diario en Linux y hay que
> compilarla desde el código. En Windows compila y muestra la cuadrícula en vivo, pero las
> grabaciones, la exportación y las fotos todavía no se han probado allí.

## Funciones

**Vista en vivo**

- Cuadrícula de cámaras de uno o más NVR, con el flujo secundario para que una cuadrícula completa
  siga siendo liviana
- Doble clic para abrir una cámara en el flujo principal, con zoom digital y gamma
- Foto guardada con la resolución del video
- Nombres de canal leídos del grabador

**Grabaciones**

- Línea de tiempo por canal para cualquier día, con los eventos de movimiento marcados aparte de la
  grabación continua
- Calendario con los días que tienen grabación
- Clic en cualquier punto de la línea de tiempo para reproducir desde ese momento, hasta 8x
- Hasta 4 canales reproducidos en sincronía
- Exportar un fragmento a MP4, sin recomprimir

La interfaz está en inglés, portugués y español, con tema oscuro y claro.

## Cómo funciona

| Qué | Cómo |
| --- | --- |
| Video en vivo | RTSP, `/cam/realmonitor?channel=N&subtype=0\|1` |
| Lista de grabaciones | Sesión de la API web (`RPC2`), o HTTP Digest con `mediaFileFind.cgi` |
| Reproducción | RTSP, `/cam/playback?channel=N&starttime=...&endtime=...`, o `loadfile.cgi` en los canales que lo rechazan |
| Exportación | HTTP Digest, `loadfile.cgi` |
| Datos del equipo y de los canales | HTTP Digest, `magicBox.cgi` y `configManager.cgi` |

GStreamer decodifica el video con aceleración por hardware cuando está disponible (VA-API en Linux,
Direct3D 11 en Windows). El núcleo está escrito en Rust y la interfaz en Qt 6 Quick.

Algunos grabadores responden 404 por RTSP para cámaras con firmware reciente, que siguen grabando
con normalidad. En esos canales, la grabación se reproduce por HTTP y el video en vivo se abre
directamente en la cámara, en la dirección que indica el grabador y con su mismo acceso.

Probado hasta ahora con los Intelbras NVD 1308 y NVD 7132. Otros grabadores basados en Dahua
deberían funcionar igual, y los informes son bienvenidos.

El acceso remoto por la nube de Intelbras (P2P) está fuera del alcance. Use una VPN para llegar al
grabador desde fuera de su red.

## Credenciales

Las contraseñas de los grabadores se guardan en el almacén del sistema (Secret Service en Linux,
Administrador de credenciales en Windows) y nunca se escriben en archivos de configuración ni en
registros.

El tráfico de video y HTTP no está cifrado, así que mantenga los grabadores en una red de confianza.
La biblioteca de RTSP responde a un servidor que pida autenticación Basic, que envía la contraseña
de forma reversible. Los grabadores usan Digest, y el riesgo es un equipo de la red que se haga
pasar por uno de ellos.

## Compilación

Se necesita Rust 1.96 o superior, Qt 6 con Qt Quick y las herramientas de Linguist, y GStreamer 1.24
o superior con los plugins base, good y bad.

```sh
cargo build --release -p rigelcam-app
```

La aplicación queda en `target/release`. En Linux, la decodificación por hardware necesita VA-API.
En Windows, compile en un entorno de Visual Studio, con `QMAKE` apuntando al `qmake` de un Qt para
MSVC, `PKG_CONFIG` al `pkg-config` del GStreamer oficial para MSVC y las carpetas `bin` de ambos en
el `PATH`.

## Contribuir

Active las comprobaciones de mensajes de commit y de formato una vez por clon:

```sh
git config core.hooksPath .githooks
```

## Licencia

[MIT](LICENSE). La fuente Space Grotesk usada en el logotipo está bajo la
[SIL Open Font License](assets/branding/SpaceGrotesk-OFL.txt).

La aplicación usa Qt y GStreamer, disponibles bajo la LGPL. Una versión distribuida debe cumplir esas
licencias, y algunos plugins de GStreamer tienen términos propios.

## Aviso

Rigelcam es un proyecto independiente, sin afiliación ni respaldo de Intelbras ni de Dahua.
Intelbras y Dahua son marcas de sus respectivos propietarios.
