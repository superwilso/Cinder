//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.0
import "qrc:/parts"
import "qrc:/components"

Item
{
  id: base

  /** type:string 音質表示文言 */
  property string sound_quality_text: ""
  /** type:bool ハイレゾか否か */
  property bool is_high_resolution: false

  width: 280
  height: 80

  SoundQualityInfo
  {
    id: sound_quality_info

    anchors { top: base.top; topMargin: 12; left: base.left; }
    width: base.width
    sound_quality_text: base.sound_quality_text
    is_high_resolution: base.is_high_resolution
  }

  SCmnLabel
  {
    id: label

    anchors { top: sound_quality_info.bottom; topMargin: 4; left: sound_quality_info.left; verticalCenter: base.verticalCenter; }
    width: base.width
    height: 26
    font.pixelSize: viewstyle.textsize.SS
    horizontalAlignment: Text.AlignHCenter
    text: qsTr("200034") + viewstyle.empty_string
  }
}
