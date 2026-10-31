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
        readonly property bool dark: grid.theme ? grid.theme === "dark" : Qt.styleHints.colorScheme !== Qt.Light
        readonly property color page: dark ? "#0B101C" : "#E9EEF6"
        readonly property color bar: dark ? "#0F1626" : "#FFFFFF"
        readonly property color surface: dark ? "#121B2F" : "#F5F8FC"
        readonly property color raised: dark ? "#1A2540" : "#E3EAF4"
        readonly property color line: dark ? "#26324D" : "#D3DCE9"
        readonly property color text: dark ? "#E4EEFF" : "#121B2F"
        readonly property color muted: dark ? "#8A94A3" : "#5B6880"
        readonly property color faint: dark ? "#5B6880" : "#7D8899"
        readonly property color accent: dark ? "#3D84F7" : "#2F6FDB"
        readonly property color accentHover: dark ? "#5A97F9" : "#3D84F7"
        readonly property color danger: dark ? "#E5484D" : "#CE2C31"
        readonly property color live: dark ? "#3DD68C" : "#1F9D63"
        readonly property color waiting: dark ? "#E5B94D" : "#A86B00"
        readonly property color panel: dark ? "#D90F1626" : "#E6FFFFFF"
        // Over the video, which looks the same in both themes.
        readonly property color overVideo: "#B30B101C"
        readonly property color overVideoText: "#E4EEFF"
    }

    component AppButton: Button {
        id: button
        property color fill: "transparent"
        property color hoverFill: colors.raised
        property color edge: "transparent"
        property color contentColor: colors.text
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
        icon.color: contentColor
        palette.buttonText: contentColor
        opacity: enabled ? 1 : 0.45
        background: Rectangle {
            radius: 7
            color: button.hovered || button.visualFocus ? button.hoverFill : button.fill
            border.color: button.edge
        }
    }

    component AppToolTip: ToolTip {
        id: tip
        delay: 400
        padding: 7
        contentItem: Text {
            text: tip.text
            font.pixelSize: 12
            color: colors.text
        }
        background: Rectangle {
            radius: 6
            color: colors.raised
            border.color: colors.line
        }
    }

    component PrimaryButton: AppButton {
        contentColor: "#FFFFFF"
        fill: colors.accent
        hoverFill: colors.accentHover
    }

    component AppComboBox: ComboBox {
        id: box
        // The text of a row, as a function so a list of labels doesn't have to be rebuilt.
        property var labelOf: item => item[textRole]
        displayText: currentIndex >= 0 ? labelOf(model[currentIndex]) : ""
        implicitHeight: 34
        font.pixelSize: 13
        font.weight: Font.Medium
        leftPadding: 0
        palette.text: colors.text
        palette.buttonText: colors.text
        background: Rectangle {
            radius: 7
            color: box.hovered ? colors.raised : colors.surface
            border.color: colors.line
        }
        contentItem: Text {
            leftPadding: 12
            rightPadding: 28
            verticalAlignment: Text.AlignVCenter
            text: box.displayText
            textFormat: Text.PlainText
            font: box.font
            color: colors.text
            elide: Text.ElideRight
        }
        indicator: Image {
            x: box.width - width - 8
            anchors.verticalCenter: parent.verticalCenter
            source: "qrc:/rigelcam/icons/chevron.svg"
            sourceSize.width: 16
            sourceSize.height: 16
        }
        delegate: ItemDelegate {
            id: boxRow
            required property var modelData
            required property int index
            width: ListView.view.width
            height: 34
            highlighted: box.highlightedIndex === index
            background: Rectangle {
                radius: 5
                color: boxRow.highlighted ? colors.raised : "transparent"
            }
            contentItem: Text {
                text: box.labelOf(boxRow.modelData)
                textFormat: Text.PlainText
                font.pixelSize: 13
                color: colors.text
                verticalAlignment: Text.AlignVCenter
            }
        }
        popup.y: box.height + 4
        popup.padding: 4
        // The default height leaves out the padding, which would make the list scroll.
        popup.height: popup.contentItem.implicitHeight + 2 * popup.padding
        popup.background: Rectangle {
            radius: 8
            color: colors.surface
            border.color: colors.line
        }
    }

    // Drags with a floating label, so the tile or row dragged stays where it is.
    component CameraDrag: DragHandler {
        property var camera
        target: null
        grabPermissions: PointerHandler.CanTakeOverFromAnything
        onActiveChanged: active ? win.beginDrag(camera, centroid.scenePosition) : win.endDrag()
        onCentroidChanged: if (active) win.moveDrag(centroid.scenePosition)
    }

    component FormField: TextField {
        id: field
        Layout.fillWidth: true
        implicitHeight: 38
        leftPadding: 12
        rightPadding: 12
        font.pixelSize: 14
        color: colors.text
        placeholderTextColor: colors.faint
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
        onTreeChanged: win.syncTiles()
        onLanguageChanged: retranslator.run()
    }

    Retranslator { id: retranslator }

    readonly property var recorderList: grid.tree ? JSON.parse(grid.tree) : []
    readonly property var gridLayout: grid.layout ? JSON.parse(grid.layout) : ({ columns: 1, cameras: [] })
    readonly property var gridKeys: gridLayout.cameras.map(c => c ? cameraKey(c.recorder, c.channel) : null)
    readonly property var tileKeys: recorderList.reduce((keys, r) => keys.concat(r.cameras.map(c => cameraKey(r.id, c.number))), [])
    readonly property int cols: gridLayout.columns
    readonly property int includedCount: gridKeys.filter(k => k !== null).length
    property int liveCount: 0
    // State of each camera in the grid, by key, as "live", "lost" or "connecting".
    property var cameraStatus: ({})
    // Key of the camera shown alone after a double click, or empty for the whole grid.
    property string zoomed: ""
    property string zoomedName: ""
    property var zoomedCamera: null
    readonly property bool fullScreen: visibility === Window.FullScreen
    // Where F11 goes back to, since the window may have been maximized or not.
    property int windowedVisibility: Window.Maximized
    onFullScreenChanged: if (fullScreen) fullScreenHint.restart()
    // Digital zoom of the camera shown alone, with the top left of the visible part in fractions.
    property real zoomScale: 1
    property real zoomX: 0
    property real zoomY: 0
    readonly property rect zoomView: Qt.rect(zoomX, zoomY, 1 / zoomScale, 1 / zoomScale)
    onZoomedChanged: {
        zoomScale = 1
        zoomX = 0
        zoomY = 0
    }
    // "live" or "recordings", which stops the live cameras to free the network and the decoder.
    property string mode: "live"
    readonly property bool inRecordings: mode === "recordings"
    // The camera being watched in the recordings: { recorder, number, name }, or null.
    property var playbackCamera: null
    // The camera being dragged to a square.
    property var dragged: null
    // Recorders with their cameras folded in the sidebar, for this session.
    property var foldedRecorders: []

    // Messages from Rust carry their arguments after the text. Reading the language makes bindings
    // to this function follow a change of it.
    function msg(text) {
        if (!text || !grid.language)
            return text
        const parts = text.split("\u001f")
        let translated = qsTranslate("Main", parts[0])
        for (let i = 1; i < parts.length; i++)
            translated = translated.arg(parts[i])
        return translated
    }

    function clamp(value, low, high) {
        return Math.min(Math.max(value, low), high)
    }

    function panTo(x, y) {
        const limit = 1 - 1 / zoomScale
        zoomX = clamp(x, 0, limit)
        zoomY = clamp(y, 0, limit)
    }

    // Keeps the point at (px, py) of the view, in fractions, under the cursor.
    function zoomAt(scale, px, py) {
        const x = zoomX + px / zoomScale
        const y = zoomY + py / zoomScale
        zoomScale = clamp(scale, 1, 8)
        panTo(x - px / zoomScale, y - py / zoomScale)
    }

    function toggleFullScreen() {
        if (fullScreen) {
            visibility = windowedVisibility
        } else {
            windowedVisibility = visibility
            visibility = Window.FullScreen
        }
    }

    function cameraKey(recorder, channel) {
        return recorder + ":" + channel
    }

    function setFolded(id, folded) {
        const others = foldedRecorders.filter(r => r !== id)
        foldedRecorders = folded ? others.concat(id) : others
    }

    // A model changed item by item, so a recorder answering doesn't restart the others' cameras.
    function syncTiles() {
        const cameras = new Map()
        for (const recorder of JSON.parse(grid.tree || "[]")) {
            for (const camera of recorder.cameras) {
                const key = cameraKey(recorder.id, camera.number)
                cameras.set(key, { key: key, recorder: recorder.id, number: camera.number, label: camera.name })
            }
        }
        const keys = new Set(cameras.keys())
        for (let i = tileModel.count - 1; i >= 0; i--) {
            const key = tileModel.get(i).key
            if (cameras.has(key))
                cameras.delete(key)
            else
                tileModel.remove(i)
        }
        for (const camera of cameras.values())
            tileModel.append(camera)
        if (!keys.has(zoomed))
            zoomed = ""
        updateLiveCount()
    }

    Component.onCompleted: win.showMaximized()

    function setMode(next) {
        if (next === mode)
            return
        zoomed = ""
        mode = next
    }

    function openPlayback(camera) {
        playbackCamera = camera
        playback.open(camera)
    }

    function updateLiveCount() {
        let live = 0
        const status = {}
        for (let i = 0; i < cameraTiles.count; i++) {
            const tile = cameraTiles.itemAt(i)
            if (!tile || !tile.included)
                continue
            if (tile.hasVideo)
                live++
            status[tile.key] = tile.hasVideo ? "live" : tile.signalLost ? "lost" : "connecting"
        }
        liveCount = live
        cameraStatus = status
    }

    function moveDrag(position) {
        const point = win.contentItem.mapFromItem(null, position.x, position.y)
        dragLabel.x = point.x - dragLabel.width / 2
        dragLabel.y = point.y - dragLabel.height / 2
    }

    function beginDrag(camera, position) {
        moveDrag(position)
        dragged = camera
    }

    function endDrag() {
        if (dragLabel.Drag.target)
            dragLabel.Drag.drop()
        dragged = null
    }

    Shortcut {
        sequence: "Esc"
        enabled: win.zoomed !== "" || win.fullScreen
        onActivated: {
            if (win.zoomed !== "")
                win.zoomed = ""
            else
                win.toggleFullScreen()
        }
    }

    // Text fields keep these keys for their cursor, so they work in the search box as usual.
    Shortcut {
        sequence: "Left"
        enabled: win.inRecordings && playback.spans.length > 0
        onActivated: playback.seek(playback.position - 10)
    }

    Shortcut {
        sequence: "Right"
        enabled: win.inRecordings && playback.spans.length > 0
        onActivated: playback.seek(playback.position + 10)
    }

    Shortcut {
        sequence: "F11"
        onActivated: win.toggleFullScreen()
    }

    header: Rectangle {
        visible: win.zoomed === "" && !win.fullScreen
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

            AppButton {
                id: sidebarButton
                visible: win.recorderList.length > 0
                display: AbstractButton.IconOnly
                icon.source: "qrc:/rigelcam/icons/sidebar.svg"
                text: grid.sidebarCollapsed ? "Mostrar câmeras" : "Esconder câmeras"
                implicitWidth: 34
                leftPadding: 9
                rightPadding: 9
                onClicked: grid.saveSidebarCollapsed(!grid.sidebarCollapsed)
                AppToolTip {
                    visible: sidebarButton.hovered
                    text: sidebarButton.text
                }
            }

            Image {
                source: colors.dark ? "qrc:/rigelcam/logo.svg" : "qrc:/rigelcam/logo-light.svg"
                fillMode: Image.PreserveAspectFit
                Layout.preferredHeight: 30
                Layout.preferredWidth: 130
                Layout.rightMargin: 12
                sourceSize.height: 60
            }

            AppButton {
                visible: win.recorderList.length > 0
                text: win.inRecordings ? qsTr("Live") : qsTr("Recordings")
                edge: colors.line
                onClicked: win.setMode(win.inRecordings ? "live" : "recordings")
            }

            Rectangle {
                visible: win.recorderList.length > 0 && !win.inRecordings
                width: 7
                height: 7
                radius: 4
                color: win.liveCount > 0 ? colors.live : colors.muted
                Layout.leftMargin: 4
            }
            Label {
                visible: win.recorderList.length > 0 && !win.inRecordings
                font.pixelSize: 13
                color: colors.muted
                text: qsTr("%1 of %2 live").arg(win.liveCount).arg(win.includedCount)
            }

            // With cameras on screen the centered message below is covered.
            Label {
                visible: win.recorderList.length > 0 && text !== ""
                Layout.leftMargin: 8
                Layout.fillWidth: true
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pixelSize: 13
                color: grid.error !== "" || grid.videoUnavailable ? colors.danger : colors.waiting
                text: win.msg(grid.error || grid.missing || grid.notice)
            }

            Item {
                visible: win.recorderList.length === 0 || (grid.error === "" && grid.missing === "" && grid.notice === "")
                Layout.fillWidth: true
            }

            AppComboBox {
                Layout.preferredWidth: 150
                visible: win.recorderList.length > 0 && !win.inRecordings
                // Fixed, because a new list would reset the current row when the language changes.
                model: [1, 2, 3, 4, 5, 6].map(columns => ({ columns: columns }))
                labelOf: item => item.columns === 1 ? qsTr("1 square") : qsTr("%1 squares").arg(item.columns * item.columns)
                valueRole: "columns"
                currentIndex: win.cols - 1
                onActivated: grid.setGridColumns(currentValue)
            }

            AppComboBox {
                Layout.preferredWidth: 136
                model: [{ code: "pt_BR", label: "Português" }, { code: "en", label: "English" }]
                textRole: "label"
                valueRole: "code"
                currentIndex: model.findIndex(l => l.code === grid.language)
                onActivated: grid.saveLanguage(currentValue)
            }

            AppButton {
                id: themeButton
                display: AbstractButton.IconOnly
                icon.source: colors.dark ? "qrc:/rigelcam/icons/sun.svg" : "qrc:/rigelcam/icons/moon.svg"
                text: colors.dark ? qsTr("Light theme") : qsTr("Dark theme")
                implicitWidth: 34
                leftPadding: 9
                rightPadding: 9
                onClicked: grid.saveTheme(colors.dark ? "light" : "dark")
                AppToolTip {
                    visible: themeButton.hovered
                    text: themeButton.text
                }
            }

            AppButton {
                id: fullScreenButton
                display: AbstractButton.IconOnly
                icon.source: "qrc:/rigelcam/icons/fullscreen.svg"
                text: qsTr("Full screen (F11)")
                implicitWidth: 34
                leftPadding: 9
                rightPadding: 9
                onClicked: win.toggleFullScreen()
                AppToolTip {
                    visible: fullScreenButton.hovered
                    text: fullScreenButton.text
                }
            }
        }
    }

    AppDialog {
        id: addDialog
        // The recorder being edited, or null when adding one.
        property var editing: null
        property bool advanced: false
        title: editing ? qsTr("Edit recorder") : qsTr("Add recorder")
        width: Math.min(440, parent.width - 32)
        closePolicy: grid.formBusy ? Popup.NoAutoClose : Popup.CloseOnEscape
        onOpened: { hostField.forceActiveFocus(); grid.formError = "" }
        onClosed: passwordField.text = ""

        readonly property bool complete: hostField.text.trim() !== "" && userField.text !== "" && (editing !== null || passwordField.text !== "")

        function openFor(recorder) {
            editing = recorder
            hostField.text = recorder ? recorder.host : ""
            nameField.text = recorder ? recorder.name : ""
            userField.text = recorder ? recorder.user : ""
            httpField.text = recorder ? recorder.httpPort : 80
            rtspField.text = recorder ? recorder.rtspPort : 554
            advanced = recorder !== null && (recorder.httpPort !== 80 || recorder.rtspPort !== 554)
            open()
        }

        function submit() {
            if (!complete || grid.formBusy)
                return
            const http = parseInt(httpField.text) || 0
            const rtsp = parseInt(rtspField.text) || 0
            if (editing)
                grid.updateRecorder(editing.id, nameField.text, hostField.text, http, rtsp, userField.text, passwordField.text)
            else
                grid.addRecorder(nameField.text, hostField.text, http, rtsp, userField.text, passwordField.text)
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 4
            FormLabel { text: qsTr("Address") }
            FormField {
                id: hostField
                placeholderText: "192.168.0.10"
                onAccepted: addDialog.submit()
                // Shows the port someone typed along with the address where it ends up.
                onEditingFinished: {
                    const port = text.trim().match(/^(?:https?:\/\/)?[^\/:@\s]+:(\d+)(?:\/.*)?$/)
                    if (port)
                        httpField.text = port[1]
                }
            }
            FormLabel { text: qsTr("Name (optional)") }
            FormField { id: nameField; placeholderText: qsTr("Uses the recorder's model"); onAccepted: addDialog.submit() }
            FormLabel { text: qsTr("User") }
            FormField { id: userField; onAccepted: addDialog.submit() }
            FormLabel { text: qsTr("Password") }
            FormField {
                id: passwordField
                echoMode: TextInput.Password
                placeholderText: addDialog.editing ? qsTr("Blank to keep the current one") : ""
                onAccepted: addDialog.submit()
            }
            AppButton {
                Layout.topMargin: 6
                Layout.leftMargin: -12
                implicitHeight: 28
                icon.source: "qrc:/rigelcam/icons/chevron.svg"
                icon.width: 14
                icon.height: 14
                text: qsTr("Advanced")
                onClicked: addDialog.advanced = !addDialog.advanced
            }
            RowLayout {
                visible: addDialog.advanced
                spacing: 8
                ColumnLayout {
                    spacing: 4
                    FormLabel { text: qsTr("HTTP port"); Layout.topMargin: 0 }
                    FormField {
                        id: httpField
                        validator: IntValidator { bottom: 1; top: 65535 }
                        onAccepted: addDialog.submit()
                    }
                }
                ColumnLayout {
                    spacing: 4
                    FormLabel { text: qsTr("RTSP port"); Layout.topMargin: 0 }
                    FormField {
                        id: rtspField
                        validator: IntValidator { bottom: 1; top: 65535 }
                        onAccepted: addDialog.submit()
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 10
                wrapMode: Text.WordWrap
                font.pixelSize: 12
                color: colors.muted
                text: qsTr("Prefer a view-only user over admin. Video travels unencrypted on the network.")
            }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 6
                visible: grid.formError !== ""
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 13
                color: colors.danger
                text: win.msg(grid.formError)
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Layout.topMargin: 14
                spacing: 8
                AppButton { text: qsTr("Cancel"); enabled: !grid.formBusy; onClicked: addDialog.close() }
                PrimaryButton {
                    text: grid.formBusy ? qsTr("Testing…") : qsTr("Test and save")
                    enabled: !grid.formBusy && addDialog.complete
                    onClicked: addDialog.submit()
                }
            }
        }

        // The form closes when saving ends without an error.
        Connections {
            target: grid
            function onFormBusyChanged() {
                if (!grid.formBusy && grid.formError === "" && addDialog.opened) {
                    addDialog.close()
                }
            }
        }
    }

    AppDialog {
        id: removeDialog
        property var recorder: null
        title: qsTr("Delete recorder")
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
                text: qsTr("Really delete %1? The saved password will be erased, and the recordings on the device stay as they are.").arg(removeDialog.recorder ? removeDialog.recorder.name : "")
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                AppButton { text: qsTr("Cancel"); onClicked: removeDialog.close() }
                AppButton {
                    text: qsTr("Delete")
                    fill: colors.danger
                    hoverFill: "#F2666A"
                    contentColor: "#FFFFFF"
                    onClicked: {
                        grid.removeRecorder(removeDialog.recorder.id)
                        removeDialog.close()
                    }
                }
            }
        }
    }

    Column {
        anchors.centerIn: parent
        spacing: 14
        visible: win.recorderList.length === 0
        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error === ""
            // The bright star of the dark theme would disappear on a light page.
            source: colors.dark ? "qrc:/rigelcam/star.svg" : "qrc:/rigelcam/icon-256.png"
            sourceSize.width: 72
            sourceSize.height: 72
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error === ""
            font.pixelSize: 18
            font.weight: Font.DemiBold
            color: colors.text
            text: qsTr("No recorders yet")
        }
        PrimaryButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error === ""
            icon.source: "qrc:/rigelcam/icons/plus.svg"
            text: qsTr("Add recorder")
            onClicked: addDialog.openFor(null)
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error !== ""
            textFormat: Text.PlainText
            font.pixelSize: 14
            color: colors.danger
            text: win.msg(grid.error)
        }
        // Only on a click, since the file may still be recoverable.
        AppButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.listCorrupted
            text: qsTr("Start over")
            edge: colors.line
            onClicked: grid.startOver()
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.missing !== ""
            textFormat: Text.PlainText
            font.pixelSize: 13
            color: grid.videoUnavailable ? colors.danger : colors.waiting
            text: win.msg(grid.missing)
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.notice !== ""
            textFormat: Text.PlainText
            font.pixelSize: 13
            color: colors.waiting
            text: win.msg(grid.notice)
        }
    }

    Rectangle {
        id: sidebar
        readonly property bool open: win.recorderList.length > 0 && !grid.sidebarCollapsed && win.zoomed === "" && !win.fullScreen
        readonly property string search: cameraSearch.text.trim().toLowerCase()
        visible: open
        width: open ? 260 : 0
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        color: colors.bar

        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 1
            color: colors.line
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 10
            spacing: 8

            RowLayout {
                Layout.leftMargin: 6
                Label {
                    Layout.fillWidth: true
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    color: colors.text
                    text: qsTr("Recorders")
                }
                AppButton {
                    id: addButton
                    display: AbstractButton.IconOnly
                    icon.source: "qrc:/rigelcam/icons/plus.svg"
                    text: qsTr("Add recorder")
                    implicitWidth: 30
                    implicitHeight: 30
                    leftPadding: 7
                    rightPadding: 7
                    onClicked: addDialog.openFor(null)
                    AppToolTip {
                        visible: addButton.hovered
                        text: addButton.text
                    }
                }
            }

            FormField {
                id: cameraSearch
                implicitHeight: 34
                font.pixelSize: 13
                placeholderText: qsTr("Search camera")
            }

            Flickable {
                id: tree
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentHeight: treeColumn.implicitHeight
                // The mouse drags cameras here, and the wheel and the scroll bar move the list.
                acceptedButtons: Qt.NoButton
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}

                Column {
                    id: treeColumn
                    width: tree.width

                    Repeater {
                        model: win.recorderList

                        Column {
                            id: recorderGroup
                            required property var modelData
                            readonly property var matches: modelData.cameras.filter(c => c.name.toLowerCase().includes(sidebar.search))
                            readonly property bool folded: sidebar.search === "" && win.foldedRecorders.includes(modelData.id)
                            visible: sidebar.search === "" || matches.length > 0
                            width: treeColumn.width

                            Rectangle {
                                width: parent.width
                                height: 34
                                radius: 5
                                color: recorderHover.hovered ? colors.raised : "transparent"

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 4
                                    anchors.rightMargin: 4
                                    spacing: 6
                                    Image {
                                        source: "qrc:/rigelcam/icons/chevron.svg"
                                        sourceSize.width: 16
                                        sourceSize.height: 16
                                        rotation: recorderGroup.folded ? -90 : 0
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: recorderGroup.modelData.name
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        font.pixelSize: 13
                                        font.weight: Font.DemiBold
                                        color: colors.text
                                    }
                                    AppButton {
                                        id: editButton
                                        visible: recorderHover.hovered
                                        display: AbstractButton.IconOnly
                                        icon.source: "qrc:/rigelcam/icons/edit.svg"
                                        icon.width: 14
                                        icon.height: 14
                                        text: qsTr("Edit recorder")
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        leftPadding: 6
                                        rightPadding: 6
                                        onClicked: addDialog.openFor(recorderGroup.modelData)
                                        AppToolTip {
                                            visible: editButton.hovered
                                            text: editButton.text
                                        }
                                    }
                                    AppButton {
                                        id: removeButton
                                        visible: recorderHover.hovered
                                        display: AbstractButton.IconOnly
                                        icon.source: "qrc:/rigelcam/icons/trash.svg"
                                        icon.width: 14
                                        icon.height: 14
                                        text: qsTr("Remove recorder")
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        leftPadding: 6
                                        rightPadding: 6
                                        onClicked: {
                                            removeDialog.recorder = recorderGroup.modelData
                                            removeDialog.open()
                                        }
                                        AppToolTip {
                                            visible: removeButton.hovered
                                            text: removeButton.text
                                        }
                                    }
                                }

                                HoverHandler { id: recorderHover }
                                // Folding waits for a second click, so a double click only opens the cameras.
                                Timer {
                                    id: foldLater
                                    interval: Qt.styleHints.mouseDoubleClickInterval
                                    onTriggered: win.setFolded(recorderGroup.modelData.id, !recorderGroup.folded)
                                }
                                TapHandler {
                                    onTapped: if (tapCount === 1) foldLater.restart()
                                    onDoubleTapped: {
                                        foldLater.stop()
                                        if (!win.inRecordings)
                                            grid.openRecorder(recorderGroup.modelData.id)
                                    }
                                }
                            }

                            Text {
                                visible: recorderGroup.modelData.state === "connecting"
                                leftPadding: 26
                                bottomPadding: 6
                                font.pixelSize: 12
                                color: colors.muted
                                text: qsTr("Connecting…")
                            }

                            Column {
                                visible: recorderGroup.modelData.state === "failed"
                                width: parent.width
                                leftPadding: 26
                                bottomPadding: 6
                                spacing: 2
                                Text {
                                    width: parent.width - parent.leftPadding
                                    wrapMode: Text.WordWrap
                                    textFormat: Text.PlainText
                                    font.pixelSize: 12
                                    color: colors.danger
                                    text: win.msg(recorderGroup.modelData.failure || "")
                                }
                                AppButton {
                                    text: qsTr("Try again")
                                    implicitHeight: 28
                                    leftPadding: 8
                                    rightPadding: 8
                                    onClicked: grid.retryRecorder(recorderGroup.modelData.id)
                                }
                            }

                            Repeater {
                                model: recorderGroup.folded ? [] : recorderGroup.matches

                                Rectangle {
                                    id: cameraRow
                                    required property var modelData
                                    readonly property string key: win.cameraKey(recorderGroup.modelData.id, modelData.number)
                                    readonly property bool onGrid: win.gridKeys.includes(key)
                                    readonly property bool isPlayback: win.inRecordings && win.playbackCamera !== null
                                        && win.playbackCamera.recorder === recorderGroup.modelData.id
                                        && win.playbackCamera.number === modelData.number
                                    // The live cameras are stopped in the recordings, so their state means nothing.
                                    readonly property string status: win.inRecordings ? "" : (win.cameraStatus[key] ?? "")
                                    readonly property var payload: ({
                                        recorder: recorderGroup.modelData.id,
                                        number: modelData.number,
                                        name: modelData.name
                                    })
                                    width: recorderGroup.width
                                    height: 30
                                    radius: 5
                                    color: rowHover.hovered ? colors.raised : "transparent"

                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 26
                                        anchors.rightMargin: 8
                                        spacing: 9
                                        Rectangle {
                                            width: 8
                                            height: 8
                                            radius: 4
                                            color: cameraRow.isPlayback ? colors.accent
                                                 : cameraRow.status === "live" ? colors.live
                                                 : cameraRow.status === "lost" ? colors.danger
                                                 : cameraRow.status === "connecting" ? colors.waiting
                                                 : "transparent"
                                            border.color: cameraRow.onGrid && !win.inRecordings ? color : cameraRow.isPlayback ? colors.accent : colors.muted
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: cameraRow.modelData.name
                                            textFormat: Text.PlainText
                                            elide: Text.ElideRight
                                            font.pixelSize: 13
                                            color: cameraRow.onGrid || cameraRow.isPlayback ? colors.text : colors.muted
                                        }
                                    }

                                    AppToolTip {
                                        visible: rowHover.hovered && !win.inRecordings
                                        text: cameraRow.status === "live" ? qsTr("Live")
                                            : cameraRow.status === "lost" ? qsTr("No signal")
                                            : cameraRow.status === "connecting" ? qsTr("Connecting…")
                                            : qsTr("Outside the grid")
                                    }

                                    HoverHandler { id: rowHover }
                                    TapHandler {
                                        onDoubleTapped: {
                                            if (win.inRecordings)
                                                win.openPlayback(cameraRow.payload)
                                            else
                                                grid.addCamera(recorderGroup.modelData.id, cameraRow.modelData.number)
                                        }
                                    }
                                    CameraDrag { camera: cameraRow.payload }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Item {
        id: cameraArea
        readonly property real cellWidth: (width - (win.cols - 1) * 2) / win.cols
        readonly property real cellHeight: (height - (win.cols - 1) * 2) / win.cols
        anchors.left: sidebar.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        visible: !win.inRecordings

        ListModel { id: tileModel }

        // One tile per camera and not per square, so moving a camera doesn't reopen its stream.
        Repeater {
            id: cameraTiles
            model: tileModel

            VideoItem {
                id: tile
                required property string key
                required property int recorder
                required property int number
                required property string label
                readonly property var payload: ({ recorder: recorder, number: number, name: label })
                readonly property int square: win.gridKeys.indexOf(key)
                readonly property bool included: square >= 0
                readonly property bool shown: included && !win.inRecordings && (win.zoomed === "" || win.zoomed === key)
                readonly property bool isMain: win.zoomed === key
                view: isMain ? win.zoomView : Qt.rect(0, 0, 1, 1)

                visible: shown
                x: isMain ? 0 : (square % win.cols) * (cameraArea.cellWidth + 2)
                y: isMain ? 0 : Math.floor(square / win.cols) * (cameraArea.cellHeight + 2)
                width: isMain ? cameraArea.width : cameraArea.cellWidth
                height: isMain ? cameraArea.height : cameraArea.cellHeight

                onStatusChanged: win.updateLiveCount()
                // Later, because this closes the tile that is emitting the signal.
                onAuthRefused: Qt.callLater(grid.recorderRefused, recorder)
                onIncludedChanged: win.updateLiveCount()

                // Hidden cameras keep running for a while, since going back to the grid right after
                // zooming is common and reconnecting all of them takes seconds.
                Timer {
                    id: stopHidden
                    interval: 20000
                    onTriggered: grid.show(tile, tile.recorder, tile.number, false, false)
                }
                function apply() {
                    if (shown) {
                        stopHidden.stop()
                        grid.show(tile, recorder, number, true, false)
                    } else if (included) {
                        stopHidden.restart()
                    } else {
                        stopHidden.stop()
                        grid.show(tile, recorder, number, false, false)
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
                            text: tile.signalLost ? qsTr("No signal") : qsTr("Connecting…")
                            font.pixelSize: 12
                            color: colors.faint
                        }
                    }
                }

                // The main stream goes over the substream, which keeps running, so zooming in and
                // out never freezes the picture. It appears when its first frame arrives.
                VideoItem {
                    id: mainStream
                    anchors.fill: parent
                    visible: hasVideo && tile.isMain
                    view: tile.view
                    onAuthRefused: Qt.callLater(grid.recorderRefused, tile.recorder)
                    Connections {
                        target: tile
                        function onIsMainChanged() {
                            grid.show(mainStream, tile.recorder, tile.number, tile.isMain, true)
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
                    color: colors.overVideo
                    Text {
                        id: cameraName
                        anchors.fill: parent
                        anchors.leftMargin: 7
                        anchors.rightMargin: 7
                        verticalAlignment: Text.AlignVCenter
                        text: tile.label
                        // The name comes from the recorder, so it can't be read as markup.
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        font.weight: Font.Medium
                        color: colors.overVideoText
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    visible: tileHover.hovered && !tile.isMain
                    color: "transparent"
                    border.color: colors.accent
                    border.width: 2
                }

                AppButton {
                    id: leaveGrid
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 6
                    visible: tileHover.hovered && !tile.isMain
                    display: AbstractButton.IconOnly
                    icon.source: "qrc:/rigelcam/icons/close.svg"
                    icon.width: 14
                    icon.height: 14
                    text: qsTr("Remove from the grid")
                    implicitWidth: 26
                    implicitHeight: 26
                    leftPadding: 6
                    rightPadding: 6
                    fill: colors.overVideo
                    contentColor: colors.overVideoText
                    onClicked: grid.removeCamera(tile.recorder, tile.number)
                    AppToolTip {
                        visible: leaveGrid.hovered
                        text: leaveGrid.text
                    }
                }

                HoverHandler {
                    id: tileHover
                    cursorShape: tile.isMain && win.zoomScale > 1 ? Qt.OpenHandCursor : Qt.ArrowCursor
                }
                WheelHandler {
                    enabled: tile.isMain
                    target: null
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: event => win.zoomAt(win.zoomScale * Math.pow(1.0015, event.angleDelta.y),
                                                 event.x / tile.width, event.y / tile.height)
                }
                DragHandler {
                    property real startX
                    property real startY
                    enabled: tile.isMain && win.zoomScale > 1
                    target: null
                    onActiveChanged: {
                        startX = win.zoomX
                        startY = win.zoomY
                    }
                    onActiveTranslationChanged: {
                        if (active)
                            win.panTo(startX - activeTranslation.x / tile.width / win.zoomScale,
                                      startY - activeTranslation.y / tile.height / win.zoomScale)
                    }
                }
                TapHandler {
                    onDoubleTapped: {
                        win.zoomedName = tile.label
                        win.zoomedCamera = tile.payload
                        win.zoomed = win.zoomed ? "" : tile.key
                    }
                }
                CameraDrag {
                    enabled: !tile.isMain
                    camera: tile.payload
                }
            }
        }

        // A number and not the list of cameras, so a drop doesn't destroy the area handling it.
        Repeater {
            model: win.recorderList.length > 0 ? win.cols * win.cols : 0

            Item {
                id: square
                required property int index
                readonly property var key: win.gridKeys[index] ?? null
                readonly property bool hasTile: key !== null && win.tileKeys.includes(key)
                visible: win.zoomed === "" && !win.inRecordings
                x: (index % win.cols) * (cameraArea.cellWidth + 2)
                y: Math.floor(index / win.cols) * (cameraArea.cellHeight + 2)
                width: cameraArea.cellWidth
                height: cameraArea.cellHeight

                Rectangle {
                    anchors.fill: parent
                    visible: !square.hasTile
                    color: colors.surface

                    // A camera the recorder stopped listing can be gone for good, and only the
                    // user knows that.
                    AppButton {
                        id: clearSquare
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 6
                        visible: square.key !== null && squareHover.hovered
                        display: AbstractButton.IconOnly
                        icon.source: "qrc:/rigelcam/icons/close.svg"
                        icon.width: 14
                        icon.height: 14
                        text: qsTr("Remove from the grid")
                        implicitWidth: 26
                        implicitHeight: 26
                        leftPadding: 6
                        rightPadding: 6
                        fill: colors.overVideo
                        contentColor: colors.overVideoText
                        onClicked: {
                            const camera = win.gridLayout.cameras[square.index]
                            grid.removeCamera(camera.recorder, camera.channel)
                        }
                        AppToolTip {
                            visible: clearSquare.hovered
                            text: clearSquare.text
                        }
                    }
                    Text {
                        anchors.centerIn: parent
                        // A camera of a recorder that is offline or hasn't answered keeps its square.
                        text: square.key === null ? qsTr("Drag a camera here") : qsTr("Waiting for the recorder")
                        font.pixelSize: 12
                        color: colors.faint
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    visible: dropArea.containsDrag
                    color: "transparent"
                    border.color: colors.accent
                    border.width: 3
                }

                // Only without a tile, since this layer is above the tiles and would take their hover.
                HoverHandler {
                    id: squareHover
                    enabled: square.key !== null && !square.hasTile
                }

                DropArea {
                    id: dropArea
                    anchors.fill: parent
                    onDropped: grid.placeCamera(win.dragged.recorder, win.dragged.number, square.index)
                }
            }
        }
    }

    Rectangle {
        visible: win.zoomed !== ""
        x: 12
        y: 12
        width: zoomBar.implicitWidth + 8
        height: 42
        radius: 9
        color: colors.panel
        border.color: colors.line
        RowLayout {
            id: zoomBar
            anchors.fill: parent
            anchors.leftMargin: 4
            anchors.rightMargin: 14
            spacing: 10
            AppButton {
                icon.source: "qrc:/rigelcam/icons/back.svg"
                text: qsTr("Back")
                onClicked: win.zoomed = ""
            }
            AppButton {
                text: qsTr("Recordings")
                onClicked: {
                    const camera = win.zoomedCamera
                    win.setMode("recordings")
                    win.openPlayback(camera)
                }
            }
            Text {
                text: win.zoomedName
                textFormat: Text.PlainText
                font.pixelSize: 13
                font.weight: Font.Medium
                color: colors.text
            }
            AppButton {
                id: zoomOut
                Layout.leftMargin: 8
                enabled: win.zoomScale > 1
                display: AbstractButton.IconOnly
                icon.source: "qrc:/rigelcam/icons/minus.svg"
                text: qsTr("Zoom out")
                implicitWidth: 34
                leftPadding: 9
                rightPadding: 9
                onClicked: win.zoomAt(win.zoomScale / 1.5, 0.5, 0.5)
                AppToolTip {
                    visible: zoomOut.hovered
                    text: zoomOut.text
                }
            }
            Text {
                text: Math.round(win.zoomScale * 100) + "%"
                horizontalAlignment: Text.AlignHCenter
                Layout.preferredWidth: 42
                font.pixelSize: 13
                color: colors.text
            }
            AppButton {
                id: zoomIn
                enabled: win.zoomScale < 8
                display: AbstractButton.IconOnly
                icon.source: "qrc:/rigelcam/icons/plus.svg"
                text: qsTr("Zoom in")
                implicitWidth: 34
                leftPadding: 9
                rightPadding: 9
                onClicked: win.zoomAt(win.zoomScale * 1.5, 0.5, 0.5)
                AppToolTip {
                    visible: zoomIn.hovered
                    text: zoomIn.text
                }
            }
        }
    }

    Item {
        id: playback
        anchors.left: sidebar.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        visible: win.inRecordings

        // The day shown, as YYYY-MM-DD on the clock of the recorder.
        property string date: ""
        property var spans: []
        property int nowSeconds: 86400
        property string today: ""
        property int rate: 1
        property bool playing: false
        // Where a pipeline was opened, and where the bar is.
        property int openedAt: 0
        property real position: 0
        // Set by a seek until the new stream speaks, so the bar never goes back by itself.
        property bool seeking: false
        property real lastElapsed: 0

        readonly property var recordings: grid.recordings ? JSON.parse(grid.recordings) : null

        onRecordingsChanged: {
            if (!recordings)
                return
            date = recordings.date
            spans = recordings.spans
            const now = recordings.now.split(" ")
            today = now[0]
            const clock = now[1].split(":")
            nowSeconds = date === today ? parseInt(clock[0]) * 3600 + parseInt(clock[1]) * 60 + parseInt(clock[2]) : 86400
            if (spans.length === 0) {
                stop()
                return
            }
            // Opens near the end of the current day, and at the start of older ones.
            const last = spans[spans.length - 1]
            seek(date === today ? Math.max(last.start, last.end - 60) : spans[0].start)
        }

        function open(camera) {
            stop()
            grid.searchRecordings(camera.recorder, camera.number, date)
        }

        function changeDay(day) {
            if (win.playbackCamera)
                grid.searchRecordings(win.playbackCamera.recorder, win.playbackCamera.number, day)
        }

        function stepDay(days) {
            const day = new Date(date + "T12:00:00")
            day.setDate(day.getDate() + days)
            changeDay(Qt.formatDate(day, "yyyy-MM-dd"))
        }

        function stop() {
            playing = false
            seeking = false
            grid.show(video, 0, 0, false, false)
        }

        // A time inside a gap goes to the next recording, and one after the last does nothing.
        function recordedTime(seconds) {
            for (const span of spans) {
                if (seconds < span.end)
                    return Math.max(seconds, span.start)
            }
            return -1
        }

        function seek(seconds) {
            const target = recordedTime(seconds)
            if (target < 0 || !win.playbackCamera)
                return
            position = target
            openedAt = Math.floor(target)
            seeking = true
            playing = true
            grid.play(video, win.playbackCamera.recorder, win.playbackCamera.number, date, openedAt, rate)
        }

        function setRate(next) {
            rate = next
            seek(position)
        }

        function togglePlaying() {
            playing = !playing
            grid.pausePlayback(video, !playing)
        }

        // The stream counts only recorded time, so its elapsed seconds walk the recordings of the
        // day to find the time on the clock, jumping over the gaps.
        function timeAfter(elapsed) {
            let time = openedAt
            let left = elapsed
            for (const span of spans) {
                if (span.end <= time)
                    continue
                time = Math.max(time, span.start)
                if (left <= span.end - time)
                    return time + left
                left -= span.end - time
                time = span.end
            }
            return time
        }

        function clockText(seconds) {
            const pad = n => String(Math.floor(n)).padStart(2, "0")
            return pad(seconds / 3600) + ":" + pad(seconds / 60 % 60) + ":" + pad(seconds % 60)
        }

        onVisibleChanged: if (!visible) stop()

        VideoItem {
            id: video
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: controls.top

            onPlaybackPosition: seconds => {
                // Reports of the stream that a seek replaced still arrive for a moment. The new
                // one counts from zero again.
                if (playback.seeking) {
                    if (seconds >= playback.lastElapsed && playback.lastElapsed > 0)
                        return
                    playback.seeking = false
                }
                playback.lastElapsed = seconds
                playback.position = playback.timeAfter(seconds)
            }
            onPlaybackEnded: playback.playing = false
        }

        Label {
            anchors.centerIn: video
            visible: !win.playbackCamera || playback.spans.length === 0
            font.pixelSize: 14
            color: colors.muted
            text: win.playbackCamera ? qsTr("No recordings on this day.") : qsTr("Double-click a camera to see its recordings.")
        }

        Rectangle {
            id: controls
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 112
            color: colors.bar

            RowLayout {
                id: playerRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 10
                spacing: 6

                AppButton {
                    enabled: win.playbackCamera !== null
                    text: "‹"
                    implicitWidth: 34
                    onClicked: playback.stepDay(-1)
                    AppToolTip { visible: parent.hovered; text: qsTr("Previous day") }
                }
                AppButton {
                    enabled: win.playbackCamera !== null
                    text: playback.date ? new Date(playback.date + "T12:00:00").toLocaleDateString(Qt.locale(grid.language), Locale.ShortFormat) : ""
                    edge: colors.line
                    onClicked: calendar.open()
                    AppToolTip { visible: parent.hovered; text: qsTr("Pick a day") }

                    Popup {
                        id: calendar
                        y: -height - 6
                        modal: true
                        dim: false
                        focus: true
                        padding: 10
                        property date shown: playback.date ? new Date(playback.date + "T12:00:00") : new Date()
                        background: Rectangle {
                            radius: 10
                            color: colors.surface
                            border.color: colors.line
                        }
                        ColumnLayout {
                            RowLayout {
                                Layout.fillWidth: true
                                AppButton {
                                    text: "‹"
                                    implicitWidth: 30
                                    onClicked: calendar.shown = new Date(calendar.shown.getFullYear(), calendar.shown.getMonth() - 1, 1)
                                }
                                Label {
                                    Layout.fillWidth: true
                                    horizontalAlignment: Text.AlignHCenter
                                    color: colors.text
                                    font.pixelSize: 13
                                    text: Qt.locale(grid.language).standaloneMonthName(calendar.shown.getMonth()) + " " + calendar.shown.getFullYear()
                                }
                                AppButton {
                                    text: "›"
                                    implicitWidth: 30
                                    onClicked: calendar.shown = new Date(calendar.shown.getFullYear(), calendar.shown.getMonth() + 1, 1)
                                }
                            }
                            MonthGrid {
                                month: calendar.shown.getMonth()
                                year: calendar.shown.getFullYear()
                                locale: Qt.locale(grid.language)
                                onClicked: date => {
                                    calendar.close()
                                    playback.changeDay(Qt.formatDate(date, "yyyy-MM-dd"))
                                }
                                delegate: Text {
                                    required property var model
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                    opacity: model.month === calendar.shown.getMonth() ? 1 : 0.35
                                    color: colors.text
                                    font.pixelSize: 13
                                    text: model.day
                                }
                            }
                        }
                    }
                }
                AppButton {
                    enabled: win.playbackCamera !== null && playback.date < playback.today
                    text: "›"
                    implicitWidth: 34
                    onClicked: playback.stepDay(1)
                    AppToolTip { visible: parent.hovered; text: qsTr("Next day") }
                }

                Item { Layout.fillWidth: true }

                AppButton {
                    enabled: playback.spans.length > 0
                    text: "-10"
                    implicitWidth: 40
                    leftPadding: 4
                    rightPadding: 4
                    onClicked: playback.seek(playback.position - 10)
                    AppToolTip { visible: parent.hovered; text: qsTr("Back 10 s") }
                }
                AppButton {
                    enabled: playback.spans.length > 0
                    text: playback.playing ? "II" : "▶"
                    implicitWidth: 40
                    leftPadding: 4
                    rightPadding: 4
                    fill: colors.raised
                    onClicked: playback.togglePlaying()
                    AppToolTip { visible: parent.hovered; text: playback.playing ? qsTr("Pause") : qsTr("Play") }
                }
                AppButton {
                    enabled: playback.spans.length > 0
                    text: "+10"
                    implicitWidth: 40
                    leftPadding: 4
                    rightPadding: 4
                    onClicked: playback.seek(playback.position + 10)
                    AppToolTip { visible: parent.hovered; text: qsTr("Forward 10 s") }
                }
                Text {
                    Layout.leftMargin: 8
                    font.pixelSize: 15
                    font.weight: Font.Medium
                    color: colors.text
                    text: playback.clockText(playback.position)
                }

                Item { Layout.fillWidth: true }

                Repeater {
                    model: [1, 2, 4, 8]
                    AppButton {
                        required property int modelData
                        enabled: playback.spans.length > 0
                        text: modelData + "x"
                        implicitWidth: 40
                        leftPadding: 4
                        rightPadding: 4
                        fill: playback.rate === modelData ? colors.raised : "transparent"
                        onClicked: playback.setRate(modelData)
                    }
                }
            }

            Item {
                id: timeline
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: playerRow.bottom
                anchors.bottom: parent.bottom
                anchors.margins: 10

                // The part of the day on screen, in seconds, which the wheel changes.
                property real viewStart: 0
                property real viewEnd: 86400
                readonly property real scale: width / (viewEnd - viewStart)
                // Marks sit on round times of the clock, so they slide with the bar when it moves.
                readonly property int tickStep: {
                    for (const step of [60, 120, 300, 600, 900, 1800, 3600, 7200, 10800]) {
                        if (step * scale >= 70)
                            return step
                    }
                    return 21600
                }
                readonly property real firstTick: Math.ceil(viewStart / tickStep) * tickStep
                readonly property int tickCount: Math.max(0, Math.floor((viewEnd - firstTick) / tickStep) + 1)

                function xOf(seconds) {
                    return (seconds - viewStart) * scale
                }

                function timeAt(x) {
                    return viewStart + x / scale
                }

                function panTo(start) {
                    const span = viewEnd - viewStart
                    viewStart = win.clamp(start, 0, 86400 - span)
                    viewEnd = viewStart + span
                }

                // A zoomed bar follows the playback until the user moves it to look elsewhere,
                // and goes back to following after the next seek.
                property bool following: true

                Connections {
                    target: playback
                    function onSeekingChanged() {
                        if (playback.seeking)
                            timeline.following = true
                    }
                    function onPositionChanged() {
                        const span = timeline.viewEnd - timeline.viewStart
                        const outside = playback.position < timeline.viewStart || playback.position > timeline.viewEnd
                        if (outside && timeline.following)
                            timeline.panTo(playback.position - span * 0.1)
                    }
                }

                Rectangle {
                    id: track
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 22
                    radius: 3
                    color: colors.surface
                    clip: true

                    Repeater {
                        model: playback.spans
                        Rectangle {
                            required property var modelData
                            x: timeline.xOf(modelData.start)
                            width: Math.max(1, (modelData.end - modelData.start) * timeline.scale)
                            height: track.height
                            color: modelData.event ? colors.waiting : colors.accent
                            opacity: 0.75
                        }
                    }

                    Repeater {
                        model: timeline.tickCount
                        Rectangle {
                            required property int index
                            x: timeline.xOf(timeline.firstTick + index * timeline.tickStep)
                            width: 1
                            height: track.height
                            color: colors.page
                            opacity: 0.5
                        }
                    }

                    // After the current moment nothing was recorded yet.
                    Rectangle {
                        x: timeline.xOf(playback.nowSeconds)
                        width: Math.max(0, track.width - x)
                        height: track.height
                        color: colors.page
                        opacity: 0.6
                    }

                    Rectangle {
                        x: timeline.xOf(playback.position) - 1
                        width: 2
                        height: track.height
                        color: colors.text
                    }
                }

                Repeater {
                    model: timeline.tickCount
                    Text {
                        required property int index
                        readonly property real time: timeline.firstTick + index * timeline.tickStep
                        y: track.height + 4
                        x: timeline.xOf(time) - implicitWidth / 2
                        visible: x >= 0 && x + implicitWidth <= timeline.width
                        font.pixelSize: 11
                        color: colors.muted
                        text: playback.clockText(time).slice(0, 5)
                    }
                }

                MouseArea {
                    id: timelineMouse
                    property real pressX: 0
                    property real pressStart: 0
                    property bool panning: false
                    anchors.fill: track
                    cursorShape: panning ? Qt.ClosedHandCursor : Qt.PointingHandCursor
                    onPressed: mouse => {
                        pressX = mouse.x
                        pressStart = timeline.viewStart
                        panning = false
                    }
                    onPositionChanged: mouse => {
                        if (Math.abs(mouse.x - pressX) > 4)
                            panning = true
                        if (panning) {
                            timeline.following = false
                            timeline.panTo(pressStart - (mouse.x - pressX) / timeline.scale)
                        }
                    }
                    // Dragging only moves the bar, so it never changes what is playing by accident.
                    onReleased: mouse => {
                        if (!panning)
                            playback.seek(timeline.timeAt(mouse.x))
                        panning = false
                    }
                    onWheel: wheel => {
                        const shift = wheel.modifiers & Qt.ShiftModifier
                        if (shift || wheel.angleDelta.x !== 0) {
                            // With Shift, some systems report the wheel as horizontal and inverted.
                            const delta = shift ? (wheel.angleDelta.y || -wheel.angleDelta.x) : wheel.angleDelta.x
                            const span = timeline.viewEnd - timeline.viewStart
                            timeline.following = false
                            timeline.panTo(timeline.viewStart - delta / 120 * span * 0.1)
                            return
                        }
                        const anchor = timeline.timeAt(wheel.x)
                        const span = timeline.viewEnd - timeline.viewStart
                        const next = win.clamp(span * Math.pow(0.9985, wheel.angleDelta.y), 300, 86400)
                        const start = win.clamp(anchor - (anchor - timeline.viewStart) * next / span, 0, 86400 - next)
                        timeline.viewStart = start
                        timeline.viewEnd = start + next
                    }
                }
            }
        }
    }

    Timer {
        id: fullScreenHint
        interval: 3000
    }

    Rectangle {
        visible: fullScreenHint.running
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        y: 14
        width: hintText.implicitWidth + 24
        height: 32
        radius: 7
        color: colors.panel
        border.color: colors.line
        Text {
            id: hintText
            anchors.centerIn: parent
            font.pixelSize: 13
            color: colors.text
            text: qsTr("F11 or Esc to leave full screen")
        }
    }

    Rectangle {
        id: dragLabel
        visible: win.dragged !== null
        z: 10
        width: dragName.implicitWidth + 24
        height: 32
        radius: 7
        color: colors.raised
        border.color: colors.accent
        Drag.active: visible
        Drag.hotSpot.x: width / 2
        Drag.hotSpot.y: height / 2
        Text {
            id: dragName
            anchors.centerIn: parent
            text: win.dragged ? win.dragged.name : ""
            textFormat: Text.PlainText
            font.pixelSize: 13
            font.weight: Font.Medium
            color: colors.text
        }
    }
}
