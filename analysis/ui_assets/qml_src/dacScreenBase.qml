//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3

import "qrc:/components"
import "qrc:/parts"
import "qrc:/framework"
import "qrc:/"
ScreenBase {
  id:dacScreenBase

  //! 公開プロパティ
  /** type:bool DAC接続状態フラグ true: DACモードで接続 */
  property bool isDacConnect : true
  /** type:string 音源の入力種別 */
  property string streamingInfo : ""

  /**
   * 音楽ライブラリトップへ戻るボタンタップ時シグナル
   */
  signal musicLibraryTopiconClicked()

  /**
   * レベル値、ピークホールド値更新関数
   * @param arg_level    レベル値
   * @param arg_peakhold ピークホールド値
   */
  function frameUpdate(arg_level_value, arg_peakhold)
  {
    peakMeter.frameUpdate(arg_level_value, arg_peakhold)
  }

  QtObject {
    id: p
    /** テキストの文言の行間の幅 */
    property int line_spacing: 36
    //! デザイン定義
    property int peakMeterAreaTopMargin: 12 + 374
    property int screenTitleLabelTopMargin: peakMeterAreaTopMargin - 246
    property string streamingInfoTextColor: "#c0a565"
    property int streamingInfoLabelWidth: 400
    property int streamingInfoLabelHeight: 36
    property int streamingInfoLabelTopMargin: 8
  }

  ScreenTitleArea {
    id: titleArea
    anchors {top: parent.top; left: parent.left}
    onMusicLibraryTopiconClicked: { dacScreenBase.musicLibraryTopiconClicked() }
    visibleMusicLibraryTopicon: true
  }

  Image {
    id: titleImage
    anchors {top: titleArea.bottom; topMargin: p.screenTitleLabelTopMargin;
        horizontalCenter: parent.horizontalCenter}
    source: "qrc:/ic_dac_title.png"
  }
  SCmnMonospaceLabel {
    id: streamingInfoLabel
    anchors {top: titleImage.bottom; topMargin: p.streamingInfoLabelTopMargin
        horizontalCenter: parent.horizontalCenter}
    width: p.streamingInfoLabelWidth
    height: p.streamingInfoLabelHeight
    text: streamingInfo
    font.pixelSize: viewstyle.textsize.L
    font.weight: Font.Light
    color: p.streamingInfoTextColor
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
  }

  Item {
    enabled: isDacConnect
    width: parent.width
    anchors { top: titleArea.bottom; horizontalCenter: parent.horizontalCenter }

    PeakMeter {
      id: peakMeter
      anchors { top: parent.top; topMargin: p.peakMeterAreaTopMargin; left: parent.left; right: parent.right; }
    }
  }

  Item {
    id:disconnectMsgOverlay
    visible:!isDacConnect

    anchors { top: titleArea.bottom; left: parent.left; right: parent.right; bottom:  parent.bottom }

    Rectangle {
      anchors.fill: parent
      color: "black"
      opacity: 0.8
    }

    SCmnMultiLineLabel {
      width: 440
      anchors.centerIn: parent
      verticalAlignment: Text.AlignVCenter
      horizontalAlignment: Text.AlignLeft
      pixelSize: viewstyle.textsize.S
      color: viewstyle.textcolor.L2
      lineHeight: p.line_spacing
      text:qsTr("110036") + viewstyle.empty_string   // 再生機器との接続が確認できません。
    }
  }
}
