//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

Item {
    id: dialogRoot

    QtObject {
        id: p
        property int upper_lower_radius : 4  // 上下丸みエリア用高さ
        property int list_width         : 361
        property int text_area_margin   : 20
        property int border_margin      : 1

        property url popup_area_center: "qrc:/assets/images/popup_area_center_4r.png"
    }

    // NoCancelダイアログ
    Rectangle {
        id: dialog

        color: "#00000000"      // 透過指定
        width: p.list_width
        height: {
            if (topText.visible)
            {
                topText.height + bottomText.height
            }else{
                bottomText.height + (p.border_margin * 2)   // 上下ボーダー幅分プラス
            }
        }
        anchors.centerIn: parent
        radius: p.upper_lower_radius

        BorderImage {
          id: centerImage
          anchors.fill: parent
          source: p.popup_area_center   // 361 x 90
          width: parent.width; height: parent.height
          border.left: p.upper_lower_radius; border.top: p.upper_lower_radius
          border.right: p.upper_lower_radius; border.bottom: p.upper_lower_radius
          horizontalTileMode: BorderImage.Repeat
        }

        // 上部テキスト
        TopText {
            id: topText
        }
        // 下部テキストライン数計算に使用
        CenterText{
            id: bottomText
            width: parent.width
            height: childrenRect.height
            anchors.top: topText.visible === true ? topText.bottom : parent.top
            anchors.topMargin: topText.visible === true ? 0 : p.border_margin
        }
    }
}
