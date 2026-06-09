//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief リストボタン用スイッチボタン
　*/
import QtQuick 2.3
import "qrc:/"

Item {
    id: base
    /** type Component スイッチボタン部分以外の部分に表示するコンポーネント */
    property Component loadComponent: sampleComponent
    /** type bool スイッチボタン以外の部分をクリックした際にスイッチボタンのON/OFFを変更するかどうかのフラグ (デフォルトはFalse) */
    property bool itemConnectable: false
    /** type bool スイッチボタンのON/OFFの状態 */
    property bool checked
    /** isEnablePressAndHold 長押しイベント有効・無効プロパティ。isEnablePressAndHoldはfalseである場合、長押しイベントを拒否する。 */
    property bool isEnablePressAndHold: false
    /**
     * Click時シグナル
     * @param Nothing
     */
    signal clicked()
    /**
     * Press時シグナル
     * @param Nothing
     */
    signal pressed()
    /**
     * Release時シグナル
     * @param Nothing
     */
    signal released()
    /**
     * Release時シグナル
     * @param Nothing
     */
    signal edgeReleased()
    /**
     * PressAndHold時シグナル
     * @param Nothing
     */
    signal pressAndHold()
    /**
     * Press時Yのシグナル
     * @param posY Changed position Y
     */
    signal pressedYPosition(int posY);
    /**
     * mouseY 変化時シグナル
     * @param posY Changed position Y
     */
    signal movedYPosition(int posY);
    /**
     * キャンセル時のシグナル(プレスしたまま画面外にドラッグ検知)
     * @param Nothing
     */
    signal edgeCanceled()

    width: 480
    height: 84

    Item {
        id: background
        anchors.fill: parent
        Image {
            id: bgImageSwitchOn
            source: "qrc:/ic_default_switch_on_list_background.png"
            anchors.centerIn: parent
        }
    }

    Loader {
        id: loader
        anchors { top: parent.top; bottom: parent.bottom; left: parent.left; }
        width: base.width - sw.width
        sourceComponent: base.loadComponent
    }
    SCmnSwitchButton {
        id: sw
        enabled: base.enabled
        checked: base.checked
        anchors { right: base.right; verticalCenter: base.verticalCenter; }
        width: base.width
        isEnablePressAndHold: base.isEnablePressAndHold
        onClicked: base.clicked()
        onPressed: base.pressed()
        onReleased: base.released()
        onEdgeReleased: base.edgeReleased()
        onPressedYPosition: base.pressedYPosition(posY)
        onPressAndHold: base.pressAndHold()
        onEdgeCanceled: base.edgeCanceled()
        onMovedYPosition: base.movedYPosition(posY)
    }

    Component {
        id: sampleComponent
        SCmnLabel {
            id: label
            text: "Switch"
            enabled: base.enabled
        }
    }
}
