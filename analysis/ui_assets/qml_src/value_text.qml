//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item
{
  /** バッテリー残量がバインドされる */
  property var param

  width:parent.width
  height:parent.height

  Item {
    width:48
    height:28
    // ボリューム数値表示
    SCmnMonospaceLabel {
      id:value_text
      anchors.verticalCenter: parent.verticalCenter
      anchors.right: parent.right
      text: qsTr("000151").replace("%s", param) + viewstyle.empty_string
      font.pixelSize: viewstyle.textsize.SSS
    }
  }
}
