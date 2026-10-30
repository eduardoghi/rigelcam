import QtQuick
import QtQuick.Window
import rigelcam.video 1.0
import rigelcam

Window {
    id: win
    width: 1280
    height: 800
    color: "#0B101C"
    title: "rigelcam"

    CameraGrid {
        id: grid
        Component.onCompleted: load()
    }

    readonly property var cameras: grid.channels ? JSON.parse(grid.channels) : []
    // Channel shown alone after a double click, or 0 for the whole grid.
    property int zoomed: 0
    readonly property int visibleCount: zoomed ? 1 : cameras.length
    readonly property int cols: Math.max(1, Math.ceil(Math.sqrt(visibleCount)))
    readonly property int rows: Math.max(1, Math.ceil(visibleCount / cols))

    Component.onCompleted: win.showMaximized()

    Text {
        anchors.centerIn: parent
        visible: grid.error !== ""
        text: grid.error
        textFormat: Text.PlainText
        color: "#E4EEFF"
    }

    Grid {
        anchors.fill: parent
        columns: win.cols
        spacing: 2

        Repeater {
            model: win.cameras

            VideoItem {
                id: tile
                required property var modelData
                readonly property bool shown: win.zoomed === 0 || win.zoomed === modelData.number
                readonly property bool isMain: win.zoomed === modelData.number

                visible: shown
                width: (win.width - (win.cols - 1) * 2) / win.cols
                height: (win.height - (win.rows - 1) * 2) / win.rows

                // Hidden cameras keep running for a while, since going back to the grid right after
                // zooming is common and reconnecting all of them takes seconds.
                Timer {
                    id: stopHidden
                    interval: 20000
                    onTriggered: grid.show(tile, tile.modelData.number, false, false)
                }
                function apply() {
                    if (shown) {
                        stopHidden.stop()
                        grid.show(tile, modelData.number, true, false)
                    } else {
                        stopHidden.restart()
                    }
                }
                Component.onCompleted: apply()
                onShownChanged: apply()

                Rectangle {
                    anchors.fill: parent
                    visible: !tile.hasVideo
                    color: "#121B2F"
                    Text {
                        anchors.centerIn: parent
                        text: tile.signalLost ? "Sem sinal" : "Conectando…"
                        color: "#8A94A3"
                    }
                }

                // The main stream goes over the substream, which keeps running, so zooming in and
                // out never freezes the picture. It appears when its first frame arrives.
                VideoItem {
                    id: mainStream
                    anchors.fill: parent
                    visible: hasVideo && tile.isMain
                    Connections {
                        target: tile
                        function onIsMainChanged() {
                            grid.show(mainStream, tile.modelData.number, tile.isMain, true)
                        }
                    }
                }

                Text {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.margins: 6
                    text: tile.modelData.name
                    // The name comes from the recorder, so it can't be read as markup.
                    textFormat: Text.PlainText
                    color: "white"
                    style: Text.Outline
                    styleColor: "black"
                }

                TapHandler {
                    onDoubleTapped: win.zoomed = win.zoomed ? 0 : tile.modelData.number
                }
            }
        }
    }
}
