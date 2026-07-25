//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2

Item {
  id: base
  property ListView target_list: null
  visible: false

  MouseArea {
    id: flick_control_mouse
    anchors.fill: parent
    drag.target: parent
    drag.minimumY: 0
    drag.maximumY: parent.height
    drag.threshold: 1
    drag.filterChildren: true
    preventStealing: true
    property real pressedPosition: 0
    property var oldTime
    onPressed: {
      target_list.flick(0, target_list.verticalVelocity)   // Press検知時にPress優位で一瞬フリックが止まるので、フリックを継続させるための処理
      pressTimer.restart()
      pressedPosition = mouseY
      oldTime = new Date().getTime()
    }
    onReleased: {
      var newTime = new Date().getTime()
      // pix/sec
      var velocity = (mouseY - pressedPosition)*1000 / Math.abs(newTime - oldTime)
      if (Math.abs(mouseY - pressedPosition) > 1)
      {
        pressTimer.stop()
        target_list.flick(0, velocity)
        pressedPosition = mouseY
        oldTime = newTime
      }
    }
  }
  Timer {
    id: pressTimer
    interval: 10
    repeat: false
    triggeredOnStart: false
    onTriggered: {
      target_list.cancelFlick()
      base.visible = false
    }
  }

  QtObject {
    id: p
    property bool flickingVertically: target_list.flickingVertically
    property bool interactive: target_list.interactive
  }
  states: [
    State {
      name: "Visible"
      when: p.flickingVertically && p.interactive
      PropertyChanges { target: base; visible: true; }
    },
    State {
      name: "Invisible"
      when: !(p.flickingVertically && p.interactive)
      PropertyChanges { target: base; visible: false; }
    }
  ]
}
