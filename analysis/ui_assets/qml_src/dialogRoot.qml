//
// Copyright 2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item {
  id: dialogRoot

  /** type OKボタンを表示するかどうかのフラグ */
  property bool ok_button_visible: true
  /**
   * OKボタンClick時シグナル
   * @param Nothing
   */
  signal confirmed()

  QtObject {
    id: p
    /** 上下丸みエリア用高さ */
    property int upper_lower_radius: 4
    /** ダイアログの横幅 */
    property int list_width: 361
    /** 区切り線のサイズ */
    property int border_margin: 1
    /** リソースURL */
    property url popup_area_center: "qrc:/assets/images/popup_area_center_4r.png"
    property url icon_path: "qrc:/assets/images/btn_popup_1button_normal.png"
    property url pressed_icon_path: "qrc:/assets/images/btn_popup_1button_pressed.png"
  }

  // LargeOkダイアログ
  Rectangle {
    id: dialog
    color: "#00000000" // 透過指定
    width: p.list_width
    height: {
      if (topText.visible) {
        topText.height + bottomText.height + okButton.height
      } else {
        bottomText.height + okButton.height + p.border_margin
      }
    }
    anchors.centerIn: parent
    radius: p.upper_lower_radius

    BorderImage {
      id: centerImage
      anchors.fill: parent
      source: p.popup_area_center
      width: parent.width; height: parent.height
      border.left: p.upper_lower_radius; border.top: p.upper_lower_radius
      border.right: p.upper_lower_radius; border.bottom: p.upper_lower_radius
      horizontalTileMode: BorderImage.Repeat
    }

    // 上部テキスト
    TopText {
      id: topText
      visible: false
    }

    // 下部テキスト (セパレート、ボーダー幅を含まないエリア)
    CenterText{
      id: bottomText
      is_force_text_area_height: true
      width: parent.width
      height: childrenRect.height
      anchors.top: topText.visible === true ? topText.bottom : parent.top
      anchors.topMargin: topText.visible === true ? 0 : p.border_margin
    }

    SCmnIcon {
      id: okButton
      visible: okButtonVisible
      width: 361 // 画像幅
      height: 90 // 画像高さ
      anchors {
        bottom: parent.bottom; left: parent.left; right: parent.right;
      }
      iconPath: p.icon_path
      pressedIconPath: p.pressed_icon_path

      SCmnLabel {
        anchors.fill: parent
        text: qsTr("000000") + viewstyle.empty_string
        color: viewstyle.textcolor.L1
        font {
          pixelSize: viewstyle.textsize.L
          weight: Font.Light
        }
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
      }

      onClicked: dialogRoot.confirmed()
    }
  }
}
