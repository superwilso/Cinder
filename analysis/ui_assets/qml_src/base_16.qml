//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.0
import "qrc:/parts"

Item
{
  id: base

  /** type:string 音質表示文言 */
  property string sound_quality_text: ""
  /** type:bool ハイレゾか否か */
  property bool is_high_resolution: false

  /**
   * シンプルモード表示処理を行う
   * @param is_blind true: シンプルモード表示 / false: 通常表示
   */
  function changeSimpleModeBlindStatus(is_blind)
  {
    simple_mode_blind.changeBlindStatus(is_blind)
  }

  /**
   * アニメーションなしシンプルモード表示処理を行う
   */
  function forceBlind()
  {
    simple_mode_blind.forceBlind()
  }

  width: 280
  height: 26

  // 品質表示
  SCmnLabel
  {
    id: sound_quality

    anchors
    {
      top: base.top
      left: base.left
      leftMargin: Math.round((base.width - (sound_quality.width + (high_resolution_icon.visible ? high_resolution_icon.width: 0))) / 2)
    }
    height: base.height
    // 中央表示するにあたって、アイコンも考慮する必要があるため
    // width は text の長さに応じて自動調整として、マージンで調整する
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    font.pixelSize: viewstyle.textsize.SS
    font.weight: Font.Normal
    text: sound_quality_text
    color: is_high_resolution ? viewstyle.textcolor.Y1 : viewstyle.textcolor.L1
  }

  Image
  {
    id: high_resolution_icon

    anchors
    {
      top: sound_quality.top
      left: sound_quality.right
    }
    visible: is_high_resolution
    source: "qrc:/ic_audio_play_hr_mark_hp_amp.png"
    smooth: false
  }

  SimpleModeBlind
  {
    id: simple_mode_blind

    anchors.fill: base
  }
}
