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

## Funciones

**Vista en vivo**

- Cuadrícula de cámaras de uno o más NVR, con el flujo secundario para que una cuadrícula completa
  siga siendo liviana
- Doble clic para abrir una cámara en el flujo principal, con zoom digital y gamma
- RTX Super Resolution y RTX HDR en Windows con tarjeta NVIDIA RTX, desactivados por defecto
- Foto guardada con la resolución del video
- Nombres de canal leídos del grabador
- Las cámaras sin permiso para el usuario aparecen así, sin nuevos intentos

**Grabaciones**

- Línea de tiempo por canal para cualquier día, con los eventos de movimiento marcados aparte de la
  grabación continua
- Calendario con los días que tienen grabación
- Clic en cualquier punto de la línea de tiempo para reproducir desde ese momento, hasta 8x
- Salto al siguiente evento de movimiento o al anterior, también con Ctrl+Derecha y Ctrl+Izquierda
- Hasta 4 canales reproducidos en sincronía, cada uno con su zoom digital
- Exportar un fragmento a MP4, sin recomprimir, solo de las grabaciones que el usuario puede
  reproducir

La interfaz está en inglés, portugués y español, con tema oscuro y claro.

## Cómo funciona

| Qué | Cómo |
| --- | --- |
| Video en vivo | RTSP, `/cam/realmonitor?channel=N&subtype=0\|1` |
| Lista de grabaciones | Sesión de la API web (`RPC2`), o HTTP Digest con `mediaFileFind.cgi` |
| Reproducción | RTSP, `/cam/playback?channel=N&starttime=...&endtime=...`, o `loadfile.cgi` en los canales que lo rechazan |
| Exportación | HTTP Digest, `loadfile.cgi` |
| Datos del equipo y de los canales | HTTP Digest, `magicBox.cgi` y `configManager.cgi` |

GStreamer decodifica el video en la tarjeta gráfica, con VA-API en Linux y Direct3D 11 en Windows,
sin decodificador por software como alternativa. El núcleo está escrito en Rust y la interfaz en Qt 6 Quick.

Algunos grabadores responden 404 por RTSP para cámaras con firmware reciente, que siguen grabando
con normalidad. En esos canales, la grabación se reproduce por HTTP y el video en vivo se abre
directamente en la cámara, en la dirección que indica el grabador y con su mismo acceso. Solo se
usa una dirección en la misma red /24 del grabador, para que la contraseña no salga de ella.

Probado con los Intelbras NVD 1308 y NVD 7132. Otros grabadores basados en Dahua
deberían funcionar igual, y los informes son bienvenidos.

El acceso remoto por la nube de Intelbras (P2P) está fuera del alcance. Use una VPN para llegar al
grabador desde fuera de su red.

## Credenciales

Las contraseñas de los grabadores se guardan en el almacén del sistema (Secret Service en Linux,
Administrador de credenciales en Windows) y nunca se escriben en archivos de configuración ni en
registros. El Flatpak no tiene acceso a ese almacén y las guarda en un archivo cifrado propio, con la
clave del portal de secretos del sistema.

En Windows, la aplicación consulta la última versión en GitHub al abrirse y muestra un aviso si hay
una nueva. No se descarga nada hasta que lo pida, y un instalador solo se ejecuta si su firma es
válida. Para desactivar la consulta, use `"check_updates": false` en `recorders.json`.

El tráfico de video y HTTP no está cifrado, así que mantenga los grabadores en una red de confianza.
La biblioteca de RTSP responde a un servidor que pida autenticación Basic, que envía la contraseña
de forma reversible. Los grabadores usan Digest, y el riesgo es un equipo de la red que se haga
pasar por uno de ellos.

## Instalación

Descarga desde la [página de versiones](https://github.com/eduardoghi/rigelcam/releases). En Windows,
el instalador actualiza la aplicación por sí solo. En Linux, instala el paquete Flatpak:

```sh
flatpak install --user rigelcam-*-linux-x86_64.flatpak
```

El paquete obtiene su runtime de Flathub y no se actualiza por sí solo, así que instala cada versión
nueva de la misma forma. Sus datos están separados de una compilación desde el código, así que vuelve
a agregar los grabadores.

## Compilación

Se necesita Rust 1.99 o superior, Qt 6 con Qt Quick y las herramientas de Linguist, y GStreamer 1.24
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

Informe los problemas de seguridad como se describe en [SECURITY.md](SECURITY.md).

## Licencia

[MIT](LICENSE). La fuente Space Grotesk usada en el logotipo está bajo la
[SIL Open Font License](assets/branding/SpaceGrotesk-OFL.txt).

La aplicación usa Qt y GStreamer, disponibles bajo la LGPL. Una versión distribuida debe cumplir esas
licencias, y algunos plugins de GStreamer tienen términos propios.

## Aviso

Rigelcam es un proyecto independiente, sin afiliación ni respaldo de Intelbras ni de Dahua.
Intelbras y Dahua son marcas de sus respectivos propietarios.
