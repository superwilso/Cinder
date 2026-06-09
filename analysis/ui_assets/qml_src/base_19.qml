//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.0
import "qrc:/components"

Item {
  id: base

  /** type:var プロパティ */
  property var properties

  width: 480
  height: 80

  objectName: "outputAudioInfo"

  /** type:int 音声出力先 ID: 0: Unknown, 1: Headphone, 2: Line Out, 3: Uac Device, 4: BT Audio */
  property int output_device_id: base.properties["outputDeviceId"]
  /** type:string USB Audio 音質表示文言 */
  property string sound_quality_text_usb_audio: base.properties["soundQualityInfo"]
  /** type:bool USB Audio ハイレゾか否か */
  property bool is_high_resolution_usb_audio: base.properties["isHighResolution"]
  /** type:string BT Audio出力先情報表示文言 */
  property string output_info_text_bt_audio: base.properties["outputInfoBtAudio"]
  /** type:bool BT Audio AVRCPが使用可能か否か */
  property bool is_available_avrcp_bt_audio: base.properties["isAvailableAvrcp"]

  /**
   * ボリューム Up 通知
   */
  signal volumeUp()
  /**
   * ボリューム Down 通知
   */
  signal volumeDown()

  /**
   * シンプルモード表示処理を行う
   * @param is_blind true: シンプルモード表示 / false: 通常表示
   */
  function changeSimpleModeBlindStatus(is_blind)
  {
    bt_audio_info.changeSimpleModeBlindStatus(is_blind)
  }

  /**
   * アニメーションなしシンプルモード表示処理を行う
   */
  function forceBlind()
  {
    bt_audio_info.forceBlind()
  }

  onOutput_device_idChanged:
  {
    switch (base.output_device_id)
    {
    case 3:
      //UAC Device接続時
      usb_audio_info.visible = true
      bt_audio_info.visible = false
      break;
    case 4:
      //BT Audio接続時
      usb_audio_info.visible = false
      bt_audio_info.visible = true
      break;
    default:
      usb_audio_info.visible = false
      bt_audio_info.visible = false
      break;
    }
  }

  USBAudioInfo
  {
    id: usb_audio_info
    visible: false
    anchors { top: parent.top; horizontalCenter: parent.horizontalCenter; }
    sound_quality_text: base.sound_quality_text_usb_audio
    is_high_resolution: base.is_high_resolution_usb_audio
  }
  BtAudioInfo
  {
    id: bt_audio_info
    visible: false
    anchors { top: parent.top; left: parent.left; }
    output_info_text: base.output_info_text_bt_audio
    is_available_avrcp: base.is_available_avrcp_bt_audio

    onVolumeUp:
    {
      base.volumeUp()
    }

    onVolumeDown:
    {
      base.volumeDown()
    }
  }
}
