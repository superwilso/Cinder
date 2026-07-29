//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"

Item {
  id: base
  anchors.fill: parent

  //! 公開プロパティ
  /** type:string アニメーション表示画像 */
  property string image_source : "AutoMute_001.png"
  /** type:bool releaseボタンのenable/disable */
  property bool is_pos_zero: false

  /**
   *　AutoMute解除ボタン押下時シグナル
   */
  signal clickReleaseAutoMutingButton()

  QtObject {
    id: p
    property int guide_area_top_margin: 44
    property int guide_area_height: 350
    property int title_height: 44
    property int title_bottom_margin: 28
    property int text_width: 440
    property int text_area_height: 252 //44 + 28 + 36 * 5
    property int lineSpacing: 36
    property int image_top_margin: 2
    property int image_bottom_margin: 74
    property int image_width: 400
    property int image_height: 160
    property int button_text_margin: 24
    property int button_height: 62
    property int button_width: 200
    property int button_bottom_margin: 108
  }

  // 背景
  Rectangle {
    id: background
    anchors.fill: parent
    color: viewstyle.bgcolor.D1
  }

  // Popup下のScreen/Windowへの入力を防ぐ
  MouseArea {
    anchors.fill: parent
  }

  Item {
    id: guide_area
    anchors {
      top: parent.top
      topMargin: p.guide_area_top_margin
      left: parent.left
      right: parent.right
    }
    height: p.guide_area_height
    Item {
      id: text_area
      height: p.text_area_height
      width: p.text_width
      anchors.centerIn: parent
      SCmnLabel {
        id: title
        anchors {
          top: parent.top
          left: parent.left
        }
        font {
          pixelSize: viewstyle.textsize.LL
          weight: Font.Light
        }
        verticalAlignment:Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        height: p.title_height
        width: p.text_width
        text: qsTr("020329") + viewstyle.empty_string
      }
      SCmnMultiLineLabel {
        id: guide
        anchors {
          top: title.bottom
          topMargin: p.title_bottom_margin
          left: parent.left
        }
        horizontalAlignment: Text.AlignHCenter
        color: viewstyle.textcolor.L2
        width: p.text_width
        pixelSize: viewstyle.textsize.S
        lineHeight: p.lineSpacing
        text: qsTr("020330") + viewstyle.empty_string
      }
    }
  }
  Image{
    id: animation_image
    anchors {
      top: guide_area.bottom
      topMargin: p.image_top_margin
      horizontalCenter: parent.horizontalCenter
    }
    width: p.image_width
    height: p.image_height
    source: "qrc:/assets/images/auto_muting/" + image_source
  }
  SCmnIcon {
    id: release_icon
    enable: is_pos_zero
    anchors {
      bottom: parent.bottom
      bottomMargin: p.button_bottom_margin
      horizontalCenter: parent.horizontalCenter
    }
    width: p.button_width
    height: p.button_height
    iconPath: "qrc:/btn_default_normal.png"
    pressedIconPath: "qrc:/btn_default_pressed.png"
    disabledIconPath: "qrc:/btn_default_disable.png"
    SCmnLabel {
      id: release_button
      enabled: is_pos_zero
      anchors {
        verticalCenter: parent.verticalCenter
        horizontalCenter: parent.horizontalCenter
      }
      horizontalAlignment: Text.AlignHCenter
      verticalAlignment: Text.AlignVCenter
      font.pixelSize: viewstyle.textsize.S
      text: qsTr("020359") + viewstyle.empty_string
    }
    onClicked: {
      base.clickReleaseAutoMutingButton()
    }
  }
}
