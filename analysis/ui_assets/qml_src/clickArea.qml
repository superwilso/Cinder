//
// Copyright 2016-2017 Sony Corporation
//

import QtQuick 2.0

Item {
  id: clickArea

  signal canceled()
  signal clicked(QtObject mouse)
  signal doubleClicked(QtObject mouse)
  signal pressAndHold(QtObject mouse)
  signal pressed(QtObject mouse)
  signal released(QtObject mouse)

  /** isEnablePressAndHold 長押しイベント有効・無効プロパティ。isEnablePressAndHoldはfalseである場合、長押しイベントを拒否する。 */
  property bool isEnablePressAndHold: false
  readonly property real mouseX: mouseArea.mouseX
  readonly property real mouseY: mouseArea.mouseY
  readonly property bool pressedButton: mouseArea.pressed

  readonly property bool containsMouse: mouseArea.containsMouse

  MouseArea {
    id: mouseArea

    anchors.fill: parent

    property int pressX
    property int pressY
    property int releaseX
    property int releaseY

    onCanceled: {
      clickArea.canceled()
    }
    onDoubleClicked: {
      if(Math.max(Math.abs(pressX - releaseX), Math.abs(pressY - releaseY)) <= 10)
      {
        clickArea.doubleClicked(mouse)
      }
    }
    onPressAndHold: {
      if (clickArea.isEnablePressAndHold)
      {
        if(Math.max(Math.abs(pressX - mouse.x), Math.abs(pressY - mouse.y)) <= 10)
        {
          clickArea.pressAndHold(mouse)
        }
      }
      else
      {
        mouse.accepted = false
      }
    }
    onPressed: {
      pressX = mouse.x
      pressY = mouse.y
      clickArea.pressed(mouse)
    }
    onReleased: {
      releaseX = mouse.x
      releaseY = mouse.y
      clickArea.released(mouse)
    }
    onClicked: {
      if(Math.max(Math.abs(pressX - releaseX), Math.abs(pressY - releaseY)) <= 10)
      {
        clickArea.clicked(mouse)
      }
    }
  }
}
