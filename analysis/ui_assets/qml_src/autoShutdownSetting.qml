//
// Copyright 2017 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"

Item {
  id: autoShutdownSetting

  property var autoShutdownList
  readonly property int labelLineHeight: 36
  signal selectAutoShutdown(int autoShutdown)

  ScreenTitleArea {
    id: title
    anchors {
      top: autoShutdownSetting.top
      left: autoShutdownSetting.left
    }
    titleText: qsTr("010127") + viewstyle.empty_string
    fontWeight: Font.Light
  }

  ListView {
    id: list
    height: (84 + 1) * 5
    anchors {
      top: title.bottom
      left: parent.left
      right: parent.right
    }
    model: autoShutdownList
    delegate: delegate
    boundsBehavior: Flickable.StopAtBounds
    clip: true
  }

  ExclusiveGroup {
    id: radioGroup
  }

  Component {
    id: delegate
    SettingItemRadio {
      title: label
      checked: isChecked
      exclusiveGroup: radioGroup
      onClicked: selectAutoShutdown(item_id)
    }
  }

  SCmnLabel {
    width: 440
    height: 120
    anchors {
      top: list.bottom
      left: autoShutdownSetting.left
      topMargin: 40
      leftMargin: 20
    }
    wrapMode: Text.WordWrap
    color: viewstyle.textcolor.L2
    font.pixelSize: viewstyle.textsize.S
    font.weight: Font.Normal
    lineHeightMode: Text.FixedHeight
    lineHeight: labelLineHeight
    text: qsTr("010129") + viewstyle.empty_string  // 一定時間利用しない状態が続くと
                                                // 自動的に電源をオフします。
  }
}
