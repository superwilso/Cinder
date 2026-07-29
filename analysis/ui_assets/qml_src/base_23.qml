//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief チェックボックス
　*/
import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2

Item {
    id: base
    /** type bool チェックボックスのON/OFFの状態 */
    property bool checked
    /** type bool テキストエリアのタッチ状態 */
    property bool textAreaPressed: false
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
     * キャンセル時シグナル(プレスしたまま画面外にドラッグ検知)
     * @param Nothing
     */
    signal edgeCanceled()
    /**
     * Press時シグナル
     * @param posY Pressed position Y
     */
    signal pressedYPosition(int posY)
    /**
     * mouseY 変化時シグナル
     * @param posY Changed position y
     */
    signal movedYPosition(int posY)

    width: 84
    height: 84

    QtObject {
        id: p
        property url btn_check_off_disable: "qrc:/assets/images/btn_check_off_disable.png"
        property url btn_check_off_normal: "qrc:/assets/images/btn_check_off_normal.png"
        property url btn_check_off_pressed: "qrc:/assets/images/btn_check_off_pressed.png"
        property url btn_check_on_disable: "qrc:/assets/images/btn_check_on_disable.png"
        property url btn_check_on_normal: "qrc:/assets/images/btn_check_on_normal.png"
        property url btn_check_on_pressed: "qrc:/assets/images/btn_check_on_pressed.png"
    }

    CheckBox {
        id: checkbox
        anchors.centerIn: parent
        checked: base.checked
        style: CheckBoxStyle {
            indicator: Rectangle {
                implicitWidth: base.width
                implicitHeight: base.height
                color: "transparent"
                Image {
                    id: checkIcon
                    anchors.centerIn: parent
                    source: {
                        if(mouse_area.pressed || base.textAreaPressed){
                            if (checkbox.checked == true) {
                                p.btn_check_on_pressed
                            } else {
                                p.btn_check_off_pressed
                            }
                        }else if (checkbox.checked == true) {
                            control.enabled ? p.btn_check_on_normal : p.btn_check_on_disable
                        } else {
                            control.enabled ? p.btn_check_off_normal : p.btn_check_off_disable
                        }
                    }
                }
            }
        }
    }
    MouseArea {
      id: mouse_area
      anchors.fill: parent
      onClicked: {
        base.clicked();
      }
//      // PressedとReleasedでは、press箇所から移動した際のreleaseが取得できず、radio.pressedをfalseにすることができないため、onPressedChangedで対応。
//      onPressedChanged: {
//        checkbox.pressed = mouse_area.pressed
//        if (pressed) base.pressed();
//        else base.released();
//      }
      onPressed: {
        base.pressed()
        base.pressedYPosition(mouse.Y)
      }
      onReleased: {
        base.released()
        base.edgeReleased()
      }
      onPressAndHold: {
        if (base.isEnablePressAndHold) {
          base.pressAndHold()
        } else {
            mouse.accepted = false
        }
      }
      onCanceled: base.edgeCanceled()
      onMouseYChanged: base.movedYPosition(mouse.y)
    }
    onCheckedChanged: {
      checkbox.checked = base.checked
    }
}
