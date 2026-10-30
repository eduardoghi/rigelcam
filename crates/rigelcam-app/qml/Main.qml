import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import rigelcam.video 1.0
import rigelcam

ApplicationWindow {
    id: win
    width: 1280
    height: 800
    color: colors.page
    title: "Rigelcam"

    QtObject {
        id: colors
        readonly property color page: "#0B101C"
        readonly property color bar: "#0F1626"
        readonly property color surface: "#121B2F"
        readonly property color raised: "#1A2540"
        readonly property color line: "#26324D"
        readonly property color text: "#E4EEFF"
        readonly property color muted: "#8A94A3"
        readonly property color accent: "#3D84F7"
        readonly property color accentHover: "#5A97F9"
        readonly property color danger: "#E5484D"
    }

    component AppButton: Button {
        id: button
        property color fill: "transparent"
        property color hoverFill: colors.raised
        property color edge: "transparent"
        implicitHeight: 34
        leftPadding: 12
        rightPadding: 12
        topPadding: 0
        bottomPadding: 0
        spacing: 8
        font.pixelSize: 13
        font.weight: Font.Medium
        icon.width: 16
        icon.height: 16
        icon.color: colors.text
        palette.buttonText: colors.text
        opacity: enabled ? 1 : 0.45
        background: Rectangle {
            radius: 7
            color: button.hovered || button.visualFocus ? button.hoverFill : button.fill
            border.color: button.edge
        }
    }

    component PrimaryButton: AppButton {
        fill: colors.accent
        hoverFill: colors.accentHover
    }

    component FormField: TextField {
        id: field
        Layout.fillWidth: true
        implicitHeight: 38
        leftPadding: 12
        rightPadding: 12
        font.pixelSize: 14
        color: colors.text
        placeholderTextColor: "#5B6880"
        selectionColor: colors.accent
        background: Rectangle {
            radius: 7
            color: colors.page
            border.color: field.activeFocus ? colors.accent : colors.line
        }
    }

    component FormLabel: Label {
        font.pixelSize: 12
        color: colors.muted
        Layout.topMargin: 6
    }

    component AppDialog: Dialog {
        id: dialog
        modal: true
        anchors.centerIn: parent
        padding: 24
        topPadding: 8
        Overlay.modal: Rectangle { color: "#B3060A12" }
        background: Rectangle {
            radius: 12
            color: colors.surface
            border.color: colors.line
        }
        header: Label {
            text: dialog.title
            textFormat: Text.PlainText
            font.pixelSize: 18
            font.weight: Font.DemiBold
            color: colors.text
            padding: 24
            bottomPadding: 8
        }
    }

    CameraGrid {
        id: grid
        Component.onCompleted: load()
        onChannelsChanged: {
            win.zoomed = 0
            win.liveCount = 0
        }
    }

    readonly property var recorderList: grid.recorders ? JSON.parse(grid.recorders) : []
    readonly property var cameras: grid.channels ? JSON.parse(grid.channels) : []
    readonly property var selectedRecorder: recorderList.find(r => r.id === grid.selected)
    property int liveCount: 0
    // Channel shown alone after a double click, or 0 for the whole grid.
    property int zoomed: 0
    readonly property var zoomedCamera: cameras.find(c => c.number === zoomed)
    readonly property int visibleCount: zoomed ? 1 : cameras.length
    readonly property int cols: Math.max(1, Math.ceil(Math.sqrt(visibleCount)))
    readonly property int rows: Math.max(1, Math.ceil(visibleCount / cols))

    Component.onCompleted: win.showMaximized()

    Shortcut {
        sequence: "Esc"
        enabled: win.zoomed !== 0
        onActivated: win.zoomed = 0
    }

    header: Rectangle {
        visible: win.zoomed === 0
        height: visible ? 56 : 0
        color: colors.bar

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: colors.line
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 14
            spacing: 12

            Image {
                source: "qrc:/rigelcam/logo.svg"
                fillMode: Image.PreserveAspectFit
                Layout.preferredHeight: 30
                Layout.preferredWidth: 130
                Layout.rightMargin: 12
                sourceSize.height: 60
            }

            ComboBox {
                id: recorderBox
                Layout.preferredWidth: 220
                implicitHeight: 34
                visible: win.recorderList.length > 0
                model: win.recorderList
                textRole: "name"
                valueRole: "id"
                currentIndex: win.recorderList.findIndex(r => r.id === grid.selected)
                onActivated: grid.selectRecorder(currentValue)
                font.pixelSize: 13
                font.weight: Font.Medium
                leftPadding: 0
                palette.text: colors.text
                palette.buttonText: colors.text
                background: Rectangle {
                    radius: 7
                    color: recorderBox.hovered ? colors.raised : colors.surface
                    border.color: colors.line
                }
                contentItem: Text {
                    leftPadding: 12
                    rightPadding: 28
                    verticalAlignment: Text.AlignVCenter
                    text: recorderBox.displayText
                    textFormat: Text.PlainText
                    font: recorderBox.font
                    color: colors.text
                    elide: Text.ElideRight
                }
                indicator: Image {
                    x: recorderBox.width - width - 8
                    anchors.verticalCenter: parent.verticalCenter
                    source: "qrc:/rigelcam/icons/chevron.svg"
                    sourceSize.width: 16
                    sourceSize.height: 16
                }
                delegate: ItemDelegate {
                    id: recorderRow
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    height: 34
                    highlighted: recorderBox.highlightedIndex === index
                    background: Rectangle {
                        radius: 5
                        color: recorderRow.highlighted ? colors.raised : "transparent"
                    }
                    contentItem: Text {
                        text: recorderRow.modelData.name
                        textFormat: Text.PlainText
                        font.pixelSize: 13
                        color: colors.text
                        verticalAlignment: Text.AlignVCenter
                    }
                }
                popup.y: recorderBox.height + 4
                popup.padding: 4
                // The default height leaves out the padding, which would make the list scroll.
                popup.height: popup.contentItem.implicitHeight + 2 * popup.padding
                popup.background: Rectangle {
                    radius: 8
                    color: colors.surface
                    border.color: colors.line
                }
            }

            AppButton {
                id: removeButton
                visible: win.recorderList.length > 0
                display: AbstractButton.IconOnly
                icon.source: "qrc:/rigelcam/icons/trash.svg"
                text: "Remover gravador"
                implicitWidth: 34
                leftPadding: 9
                rightPadding: 9
                Layout.leftMargin: -6
                onClicked: removeDialog.open()
                ToolTip {
                    visible: removeButton.hovered
                    delay: 400
                    text: removeButton.text
                    padding: 7
                    contentItem: Text {
                        text: removeButton.text
                        font.pixelSize: 12
                        color: colors.text
                    }
                    background: Rectangle {
                        radius: 6
                        color: colors.raised
                        border.color: colors.line
                    }
                }
            }

            Rectangle {
                visible: win.cameras.length > 0
                width: 7
                height: 7
                radius: 4
                color: win.liveCount > 0 ? "#3DD68C" : colors.muted
                Layout.leftMargin: 4
            }
            Label {
                visible: win.cameras.length > 0
                font.pixelSize: 13
                color: colors.muted
                text: win.liveCount + " de " + win.cameras.length + " ao vivo"
            }

            Item { Layout.fillWidth: true }

            PrimaryButton {
                icon.source: "qrc:/rigelcam/icons/plus.svg"
                text: "Adicionar gravador"
                onClicked: addDialog.open()
            }
        }
    }

    AppDialog {
        id: addDialog
        title: "Adicionar gravador"
        width: Math.min(440, parent.width - 32)
        closePolicy: grid.formBusy ? Popup.NoAutoClose : Popup.CloseOnEscape
        onOpened: { hostField.forceActiveFocus(); grid.formError = "" }
        onClosed: passwordField.text = ""

        function submit() {
            if (hostField.text.trim() && userField.text && passwordField.text && !grid.formBusy)
                grid.addRecorder(nameField.text, hostField.text, userField.text, passwordField.text)
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 4
            FormLabel { text: "Endereço" }
            FormField { id: hostField; placeholderText: "192.168.0.10"; onAccepted: addDialog.submit() }
            FormLabel { text: "Nome (opcional)" }
            FormField { id: nameField; placeholderText: "Usa o modelo do gravador"; onAccepted: addDialog.submit() }
            FormLabel { text: "Usuário" }
            FormField { id: userField; onAccepted: addDialog.submit() }
            FormLabel { text: "Senha" }
            FormField { id: passwordField; echoMode: TextInput.Password; onAccepted: addDialog.submit() }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 10
                wrapMode: Text.WordWrap
                font.pixelSize: 12
                color: colors.muted
                text: "Prefira um usuário só de visualização em vez do admin. O vídeo trafega sem criptografia na rede."
            }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 6
                visible: grid.formError !== ""
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 13
                color: colors.danger
                text: grid.formError
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Layout.topMargin: 14
                spacing: 8
                AppButton { text: "Cancelar"; enabled: !grid.formBusy; onClicked: addDialog.close() }
                PrimaryButton {
                    text: grid.formBusy ? "Testando…" : "Testar e salvar"
                    enabled: !grid.formBusy && hostField.text.trim() !== "" && userField.text !== "" && passwordField.text !== ""
                    onClicked: addDialog.submit()
                }
            }
        }

        // The form closes when saving ends without an error.
        Connections {
            target: grid
            function onFormBusyChanged() {
                if (!grid.formBusy && grid.formError === "" && addDialog.opened) {
                    hostField.text = ""
                    nameField.text = ""
                    userField.text = ""
                    addDialog.close()
                }
            }
        }
    }

    AppDialog {
        id: removeDialog
        title: "Excluir gravador"
        width: Math.min(400, parent.width - 32)
        ColumnLayout {
            anchors.fill: parent
            spacing: 18
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 14
                color: colors.muted
                text: "Quer mesmo excluir " + (win.selectedRecorder ? win.selectedRecorder.name : "") + "? A senha salva será apagada, e as gravações no aparelho não mudam."
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                AppButton { text: "Cancelar"; onClicked: removeDialog.close() }
                AppButton {
                    text: "Excluir"
                    fill: colors.danger
                    hoverFill: "#F2666A"
                    onClicked: {
                        grid.removeRecorder(grid.selected)
                        removeDialog.close()
                    }
                }
            }
        }
    }

    Column {
        anchors.centerIn: parent
        spacing: 14
        visible: win.cameras.length === 0
        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: win.recorderList.length === 0 && grid.error === ""
            source: "qrc:/rigelcam/star.svg"
            sourceSize.width: 72
            sourceSize.height: 72
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: win.recorderList.length === 0 && grid.error === ""
            font.pixelSize: 18
            font.weight: Font.DemiBold
            color: colors.text
            text: "Nenhum gravador cadastrado"
        }
        PrimaryButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: win.recorderList.length === 0 && grid.error === ""
            icon.source: "qrc:/rigelcam/icons/plus.svg"
            text: "Adicionar gravador"
            onClicked: addDialog.open()
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: win.recorderList.length > 0 && grid.error === ""
            font.pixelSize: 14
            color: colors.muted
            text: "Conectando ao gravador…"
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error !== ""
            textFormat: Text.PlainText
            font.pixelSize: 14
            color: colors.danger
            text: grid.error
        }
    }

    Grid {
        id: cameraArea
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
                width: (cameraArea.width - (win.cols - 1) * 2) / win.cols
                height: (cameraArea.height - (win.rows - 1) * 2) / win.rows

                onHasVideoChanged: win.liveCount += hasVideo ? 1 : -1

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
                    color: colors.surface
                    Column {
                        anchors.centerIn: parent
                        spacing: 6
                        Image {
                            anchors.horizontalCenter: parent.horizontalCenter
                            visible: tile.signalLost
                            source: "qrc:/rigelcam/icons/camera-off.svg"
                            sourceSize.width: 24
                            sourceSize.height: 24
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: tile.signalLost ? "Sem sinal" : "Conectando…"
                            font.pixelSize: 12
                            color: "#5B6880"
                        }
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

                // At the top because recorders usually print their own title at the bottom left.
                Rectangle {
                    visible: !tile.isMain
                    x: 6
                    y: 6
                    width: Math.min(cameraName.implicitWidth + 14, tile.width - 12)
                    height: cameraName.implicitHeight + 6
                    radius: 5
                    color: "#B30B101C"
                    Text {
                        id: cameraName
                        anchors.fill: parent
                        anchors.leftMargin: 7
                        anchors.rightMargin: 7
                        verticalAlignment: Text.AlignVCenter
                        text: tile.modelData.name
                        // The name comes from the recorder, so it can't be read as markup.
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        font.weight: Font.Medium
                        color: colors.text
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    visible: tileHover.hovered && !tile.isMain
                    color: "transparent"
                    border.color: colors.accent
                    border.width: 2
                }

                HoverHandler { id: tileHover }
                TapHandler {
                    onDoubleTapped: win.zoomed = win.zoomed ? 0 : tile.modelData.number
                }
            }
        }
    }

    Rectangle {
        visible: win.zoomed !== 0
        x: 12
        y: 12
        width: zoomBar.implicitWidth + 8
        height: 42
        radius: 9
        color: "#D90F1626"
        border.color: colors.line
        RowLayout {
            id: zoomBar
            anchors.fill: parent
            anchors.leftMargin: 4
            anchors.rightMargin: 14
            spacing: 10
            AppButton {
                icon.source: "qrc:/rigelcam/icons/back.svg"
                text: "Voltar"
                onClicked: win.zoomed = 0
            }
            Text {
                text: win.zoomedCamera ? win.zoomedCamera.name : ""
                textFormat: Text.PlainText
                font.pixelSize: 13
                font.weight: Font.Medium
                color: colors.text
            }
        }
    }
}
