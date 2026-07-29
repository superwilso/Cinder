//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief ラジオボタン
 * 複数のラジオボタンを同一グループにするためには、ExclusiveGroupを設定する必要があります。
　*/
import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2

Item {
    id: base
    /** type ExclusiveGroup ラジオボタングループ */
    property ExclusiveGroup exclusiveGroup: null
    /** type bool ラジオボタンの選択状態 */
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
     * Press and hold時シグナル
     * @param Nothing
     */
    signal pressAndHold()
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

    // The width of the radio button
    width: 84
    // The height of the radio button
    height: 84

    property url btn_radio_off_disable: "qrc:/assets/images/btn_radio_off_disable.png"
    property url btn_radio_off_normal:  "qrc:/assets/images/btn_radio_off_normal.png"
    property url btn_radio_off_pressed: "qrc:/assets/images/btn_radio_off_pressed.png"
    property url btn_radio_on_disable:  "qrc:/assets/images/btn_radio_on_disable.png"
    property url btn_radio_on_normal:   "qrc:/assets/images/btn_radio_on_normal.png"
    property url btn_radio_on_pressed:  "qrc:/assets/images/btn_radio_on_pressed.png"

    RadioButton {
        id: radio
        enabled: base.enabled
        exclusiveGroup: base.exclusiveGroup
        checked: base.checked
        style: RadioButtonStyle {
            indicator: Rectangle {
                implicitWidth: base.width
                implicitHeight: base.height
                color: "transparent"
                Image {
                    id: checkIcon
                    anchors.right: parent.right
                    source: {
                        if(radio.pressed || base.textAreaPressed){
                            if (radio.checked == true) {
                                base.btn_radio_on_pressed
                            } else {
                                base.btn_radio_off_pressed
                            }
                        }else if (radio.checked == true) {
                            control.enabled ? base.btn_radio_on_normal : base.btn_radio_on_disable
                        } else {
                            control.enabled ? base.btn_radio_off_normal : base.btn_radio_off_disable
                        }
                    }
                }
            }
        }
    }
    MouseArea {
      id: mouse
      anchors.fill: parent
      onClicked: {
        base.clicked()
      }
      // PressedとReleasedでは、press箇所から移動した際のreleaseが取得できず、radio.pressedをfalseにすることができないため、onPressedChangedで対応。
      onPressedChanged: {
        radio.pressed = mouse.pressed
        if (pressed) base.pressed();
        else base.released();
      }
      onPressed: {
          base.pressed()
          base.pressedYPosition(mouse.y)
      }
      onPressAndHold: {
          if (base.isEnablePressAndHold) {
              base.pressAndHold();
          } else {
              mouse.accepted = false;
          }
      }
      onReleased: {
          base.released()
          base.edgeReleased()
      }
      onCanceled: base.edgeCanceled()
      onMouseYChanged: base.movedYPosition(mouse.y)
      onExited: {
        base.released();
        radio.pressed = false
      }
    }
    onCheckedChanged: {
        radio.checked = base.checked;
    }
}

