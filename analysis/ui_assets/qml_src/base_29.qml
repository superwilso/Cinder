//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief 汎用プルダウンリスト
　*/
import QtQuick 2.3
import "qrc:/framework"

Item {
    id: base
    /** PullDownListDialog　に表示するデータ  */
    property var listData
    /** プルダウンリストの選択項目  */
    property string currentText: "test"
    /** プルダウンリストに表示するタイトル  */
    property string dialogTitle: "type"
    /** プルダウンボタンの幅  */
    property int pulldownWidth: 480
    /** プルダウンボタンの高さ  */
    property int pulldownHeight: 84
    /** プルダウンボタンタイトルの幅 */
    property int currentTextWidth: 370
    /** プルダウンボタンテキストサイズ */
    property int currentTextPixelSize: viewstyle.textsize.L
    /** プルダウンボタンEnable状態時テキストカラー */
    property var enabledTextColor: viewstyle.textcolor.L1
    /** プルダウンボタンDisable状態時テキストカラー */
    property var disabledTextColor: viewstyle.textcolor.L3
    /** OKボタン有無 */
    property bool okButton: true
    /** isEnablePressAndHold 長押しイベント有効・無効プロパティ。isEnablePressAndHoldはfalseである場合、長押しイベントを拒否する。 */
    property bool isEnablePressAndHold: false

    /**
     * プルダウン選択時シグナル
     * @param index
     */
    signal opened()
    signal selected(var item_id)
    signal accepted()
    signal closed()
    signal edgeReleased()
    signal edgeCanceled()
    signal pressedYPosition(int posY)
    signal movedYPosition(int posY)
    /**
     * PressAndHold時シグナル
     * @param Nothing
     */
    signal pressAndHold();

    QtObject {
        id: p
        property var startItemId
        property var select_item_id
        /** componentのマウスエリアがpressed状態か否か */
        property bool isComponentMouseAreaPressed: false
    }

    states: [
        State {
            name: "normal"
            when: base.enabled && !p.isComponentMouseAreaPressed
            PropertyChanges {
                target: pullDownListImage
                source: "qrc:/assets/images/btn_dropdown_normal.png"
            }
        }, State {
            name: "pressed"
            when: base.enabled && p.isComponentMouseAreaPressed
            PropertyChanges {
                target: pullDownListImage
                source: "qrc:/assets/images/btn_dropdown_pressed.png"
            }
        }, State {
            name: "disable"
            when: !base.enabled
            PropertyChanges {
                target: pullDownListImage
                source: "qrc:/assets/images/btn_dropdown_disable.png"
            }
        }
    ]

    Image {
        id: pullDownListImage
        width: pulldownWidth
        height: pulldownHeight
        source: ""
        SCmnLabel {
            width: base.currentTextWidth
            height: parent.height
            anchors.centerIn: parent
            text: currentText
            horizontalAlignment: Text.AlignLeft
            verticalAlignment: Text.AlignVCenter
            color: base.enabled ? base.enabledTextColor : base.disabledTextColor
            font.pixelSize: base.currentTextPixelSize
        }
        MouseArea {
            anchors.fill: parent
            onClicked: {
                dialogOpen(dialogTitle);
            }
            onPressed: {
                p.isComponentMouseAreaPressed = true;
                base.pressedYPosition(mouse.y);
            }
            onReleased: {
                p.isComponentMouseAreaPressed = false;
                base.edgeReleased();
            }
            onCanceled: {
                p.isComponentMouseAreaPressed = false;
                base.edgeCanceled();
            }
            onMouseYChanged: base.movedYPosition(mouse.y)
            onPressAndHold: {
                if (base.isEnablePressAndHold) {
                    base.pressAndHold()
                } else {
                    mouse.accepted = false
                }
            }
        }
    }

    /**
     * ダイアログオープン
     * @return nothing
     */
    function dialogOpen(dialogTitle) {
        p.startItemId = listData.checkedItemId
        p.select_item_id = listData.checkedItemId
        if (okButton) {
            dialog.dialogOpen("kMid9213", dialogTitle, "", listData)
        } else {
            dialog.dialogOpen("kMid9214", dialogTitle, "", listData)
        }
        base.opened()
    }
    // 割り込みによるダイアログクローズ
    function dialogInterruptClose() {
        if(("kMid9213" === dialog.getMessageID()) || ("kMid9214" === dialog.getMessageID())){
            dialog.close()
        }
    }

    // Dialog1を表示した場合のDialogManagerからのダイアログクローズ受信
    DialogWrapper {
        id: dialog
        visible: false
        onAccepted: {
            base.accepted()
        }
        onSelected: {
            p.select_item_id = item_id
            base.selected((parseInt)(item_id))
        }
        onClosed: {
            if(p.startItemId !== p.select_item_id){
                base.selected(p.startItemId)
            }
            base.closed()
        }
    }
}
