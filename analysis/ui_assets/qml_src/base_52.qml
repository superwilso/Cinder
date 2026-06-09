//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/sid_0520_usb_audio_connecting"
import "qrc:/"

SoundSettingBase
{
  id: base
  visible: true

  /** type:bool DSEE AI ON/OFF設定値 */
  property bool is_dsee_ai_on: true
  /** type:string DSEE AI 説明文 */
  property string dsee_ai_explanation
  /**
   * DSEE AI ON/OFF設定値変更
   * @param is_dsee_ai_on true: 設定ON, false: 設定OFF
   */
  signal isDseeAiChange(bool is_dsee_ai_on)

  QtObject
  {
    id: p
    readonly property int text_width: 440

    readonly property int switch_text_width: 156
    readonly property int switch_text_left_margin: 102

    readonly property int type_image_width: 480
    readonly property int type_image_height: 240
    readonly property string dsee_ai_image_path: "qrc:/ic_se_setting_dsee_AI_tutorial.png"

    readonly property int explanation_height: 180
    readonly property int explanation_line_spacing: 34
    readonly property int explanation_top_margin: 8

    readonly property int text_maximum_line_count: 3

    property real visible_opacity: 1.0
    property real invisible_opacity: 0.0
    property int fade_duration: 100
  }
  // 画面背景
  Rectangle
  {
    id: background
    anchors.fill: parent
    color: viewstyle.bgcolor.D1
  }
  // 画面ヘッダ
  SoundSettingHeader
  {
    id: sound_setting_header
    anchors
    {
      top: base.top;
    }
    is_source_direct: base.is_source_direct
    is_clear_audio_plus: base.is_clear_audio_plus
    title: base.title
  }
  // DSEE AI ON/OFF切り替えスイッチラベル
  SCmnLabel
  {
    id: switch_text
    width: p.switch_text_width
    anchors
    {
      left: parent.left;
      verticalCenter: dsee_ai_switch.verticalCenter;
      leftMargin: p.switch_text_left_margin;
    }
    text: (dsee_ai_switch.checked ? qsTr("000042") : qsTr("000043")) + viewstyle.empty_string // ON or OFF
    verticalAlignment: Text.AlignVCenter
    horizontalAlignment: Text.AlignHCenter
    color: viewstyle.textcolor.L1
    font
    {
      pixelSize: viewstyle.textsize.L;
      weight: Font.Light;
    }
  }
  // DSEE AI ON/OFF切り替えスイッチ
  SCmnSwitchButton
  {
    id: dsee_ai_switch
    enabled: !(base.usb_connect || base.is_source_direct || base.is_clear_audio_plus)
    anchors
    {
      top: type_image.bottom;
      left: switch_text.right;
    }
    checked: base.is_dsee_ai_on
    onClicked: base.isDseeAiChange(!base.is_dsee_ai_on)
  }
  onIs_dsee_ai_onChanged:
  {
    dsee_ai_switch.checked = base.is_dsee_ai_on
  }
  // 機能イメージ
  Image
  {
    id: type_image
    anchors
    {
      top: sound_setting_header.bottom;
      left: parent.left;
    }
    width: p.type_image_width
    height: p.type_image_height
    source: p.dsee_ai_image_path
    fillMode: Image.PreserveAspectFit
  }
  // 機能説明文
  SCmnLabel
  {
    id: explanation
    width: p.text_width
    anchors
    {
      top: type_image.top;
      topMargin: p.explanation_top_margin;
      horizontalCenter: parent.horizontalCenter;
    }
    height: p.explanation_height
    text: base.dsee_ai_explanation
    font
    {
      pixelSize: viewstyle.textsize.S;
      weight: Font.Light;
    }
    color: viewstyle.textcolor.L1
    wrapMode: Text.WordWrap
    lineHeightMode: Text.FixedHeight
    lineHeight: p.explanation_line_spacing
    horizontalAlignment: Text.AlignLeft
    verticalAlignment: Text.AlignTop
    maximumLineCount: p.text_maximum_line_count
  }
  // ソースダイレクト中/ClearAudio+中/USB接続中のMouseイベント無効化
  MouseArea
  {
    id: on_source_clear
    anchors.fill: parent
    enabled: (base.usb_connect || base.is_source_direct || base.is_clear_audio_plus)
  }
  // スワイプ制御
  SwipeSelectorForLibrary
  {
    id: swipe_selector
    anchors.fill: on_source_clear
    objectName: "swipeSelector"
    enabled: on_source_clear.enabled
  }
  // ソースダイレクト中/ClearAudio+中表示
  SrcDirectOrClearAudioDialog
  {
    id: src_or_clear_dialog
    anchors
    {
      top: sound_setting_header.bottom;
      left: base.left;
    }
    stateVisible: base.is_source_direct || base.is_clear_audio_plus
    is_available_clear_audio_plus: base.is_available_clear_audio_plus
    is_available_source_direct: base.is_available_source_direct
  }
  // USB接続中表示
  UsbAudioConnecting
  {
    id: usbAudioConnectingDialog
    anchors
    {
      top: parent.top;
      left: parent.left;
    }
    dialog_open: base.usb_connect
    dialog_animation: base.usb_dialog_animation
  }
  // フェード処理
  states: [
    State {
      when: !src_or_clear_dialog.stateVisible && !usbAudioConnectingDialog.dialog_open
      PropertyChanges { target: switch_text; opacity: p.visible_opacity }
      PropertyChanges { target: dsee_ai_switch; opacity: p.visible_opacity }
      PropertyChanges { target: type_image; opacity: p.visible_opacity }
      PropertyChanges { target: explanation; opacity: p.visible_opacity }
    },
    State {
      when: src_or_clear_dialog.stateVisible || usbAudioConnectingDialog.dialog_open
      PropertyChanges { target: switch_text; opacity: p.invisible_opacity }
      PropertyChanges { target: dsee_ai_switch; opacity: p.invisible_opacity }
      PropertyChanges { target: type_image; opacity: p.invisible_opacity }
      PropertyChanges { target: explanation; opacity: p.invisible_opacity }
    }
  ]
  transitions: Transition {
      NumberAnimation { property: "opacity"; duration: p.fade_duration }
  }
}
