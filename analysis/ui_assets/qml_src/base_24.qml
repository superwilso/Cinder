//
// Copyright 2018 Sony Corporation
//

/**
 * @brief 左側にラジオボタン、右側にテキストを表示するラジオボタン
 * 複数のラジオボタンを同一グループにするためには、ExclusiveGroupを設定する必要があります。
　*/
import QtQuick 2.3
import QtQuick.Controls 1.2

Item {
  id: base
  /** type Component ラジオボタン以外の部分に表示するコンポーネント */
  property Component loadComponent
  /** type ExclusiveGroup ラジオボタングループ */
  property ExclusiveGroup exclusiveGroup: null
  /** type bool ラジオボタン以外の部分をクリックした際にラジオボタンのON/OFFを変更するかどうかのフラグ (デフォルトはTrue) */
  property bool itemConnectable: true
  /** type bool ラジオボタンのON/OFFの状態 */
  property bool checked
  /** isEnablePressAndHold 長押しイベント有効・無効プロパティ。isEnablePressAndHoldはfalseである場合、長押しイベントを拒否する。 */
  property bool isEnablePressAndHold: false
  /** MouseAreaのClickedと判定するPressed位置とReleased位置の距離。 */
  /** この距離より短い場合にClickedと判定する。単位はpx(ピクセル) */
  readonly property real clicked_distance: 5.0
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
     * press and hold時シグナル
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

  width: 480
  height: 50

  Item {
    id: itemArea
    anchors.fill: parent
    Item {
      id: radioArea
        anchors { top: parent.top; bottom: parent.bottom; left: parent.left; right: loader.left}
        width: base.height
        SCmnRadioButton {
          id: radio
          anchors.left: parent.left
          exclusiveGroup: base.exclusiveGroup
          checked: base.checked
          isEnablePressAndHold: base.isEnablePressAndHold
          onClicked: base.clicked()
          onPressed: base.pressed()
          onPressAndHold: base.pressAndHold()
          onPressedYPosition: base.pressedYPosition(posY)
          onReleased: base.released()
          onEdgeReleased: base.edgeReleased()
          onEdgeCanceled: base.edgeCanceled()
          onMovedYPosition: base.movedYPosition(posY)
        }
      }
    Loader {
      id: loader
      anchors { top: parent.top; bottom: parent.bottom; right: parent.right; }
      sourceComponent: base.loadComponent
      MouseArea {
        id: mouseArea
        anchors.fill: parent
        property real pressed_x: 0.0
        property real released_x: 0.0
        property real pressed_y: 0.0
        property real released_y: 0.0
        onClicked: {
          if (itemConnectable) {
            if (!radio.checked){
              if ((Math.abs(pressed_x - released_x) < base.clicked_distance) &&
                    (Math.abs(pressed_y - released_y) < base.clicked_distance))
              {
                base.clicked()
              }
            }
          }
        }
        onPressed: {
          pressed_x = mouseX
          pressed_y = mouseY
          base.pressed()
          base.pressedYPosition(mouse.y)
        }
        onPressAndHold: {
          if (base.isEnablePressAndHold) {
            base.pressAndHold()
          } else {
            mouse.accepted = false
          }
        }
        onPressedChanged: {
          if (itemConnectable) {
            // Text Area press状態をRadioButtonに伝える
            radio.textAreaPressed = pressed
          }
        }
        onReleased: {
          released_x = mouseX
          released_y = mouseY
          base.released()
          base.edgeReleased()
        }
        onCanceled: base.edgeCanceled()
        onMouseYChanged: base.movedYPosition(mouse.y)
      }
    }
  }
  onCheckedChanged: {
    radio.checked = base.checked;
  }
}
