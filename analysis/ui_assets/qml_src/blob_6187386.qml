//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.0

MouseArea {

    anchors.fill: parent

    signal pressedSelector()
    signal releasedSelector()
    signal pressAndHoldSelector()

    signal startedSwipe(int direction)
    signal finishedSwipe(bool completed)

    property var properties

    property QtObject target

    objectName: properties["objectName"]

    onPressed: {

      pressedSelector();

      mouse.accepted = false
    }
    onReleased: {

      releasedSelector();

      mouse.accepted = false
    }
    onPressAndHold: {

      pressAndHoldSelector();

      mouse.accepted = false;
    }

    onFinishedSwipe: {

      // スワイプ時、targetへの操作をキャンセルする
      if(target)
      {
        target.enabled  = false
        target.enabled  = true
      }
    }
}
