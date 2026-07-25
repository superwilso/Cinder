//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.0

ClickArea {
  id: imageButton

  property string normalImage:  properties["normalImage"]
  property string disableImage: properties["disableImage"]
  property string pressedImage: properties["pressedImage"]

  property bool pressing: false

  property var properties

  state: "Enabled"

  enabled: properties["Enabled"]
  visible: properties["Visible"]

  onEnabledChanged: {

    changeState()
  }

  onPressed: {

    pressing = true

    changeState()
  }

  onReleased: {

    pressing = false

    changeState()
  }

  function changeState() {

    state = enabled ? (pressing ? "Pressed" : "Enabled") : "Disabled"
  }

  Image {
    id: buttonImage

    anchors.centerIn: parent

    source: normalImage
  }

  Rectangle {

    anchors.fill: buttonImage

    color: "black"

    opacity: ((disableImage.length > 0) || imageButton.enabled) ? 0.0 : 0.7
  }

  states: [
    State {
      name: "Enabled"
      PropertyChanges {
        target: buttonImage
        source: normalImage
      }
    },
    State {
      name: "Pressed"
      PropertyChanges {
        target: buttonImage
        source: pressedImage
      }
    },
    State {
      name: "Disabled"
      PropertyChanges {
        target: buttonImage
        source: disableImage.length > 0 ? disableImage : normalImage
      }
    }
  ]

}

