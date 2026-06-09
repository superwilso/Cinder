//
// Copyright 2017 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

Rectangle {
    color: viewstyle.bgcolor.D1

    property string guideText

    BorderImage {
        source: "qrc:/assets/images/popup_area_center_4r.png"
        width: 400
        height: 180
        border.left: 4
        border.top: 4
        border.right: 4
        border.bottom: 4
        anchors {
            centerIn: parent
        }

        SCmnLabel {
            width: 360
            height: 160
            anchors {
                top: parent.top
                left: parent.left
                topMargin: 10
                bottomMargin: 10
                leftMargin: 20
                rightMargin: 20
            }
            lineHeight: 32
            lineHeightMode: Text.FixedHeight
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
            font {
                pixelSize: viewstyle.textsize.S
                weight: Font.Regular
            }
            wrapMode: Text.Wrap
            maximumLineCount: 5
            color: viewstyle.textcolor.L2
            text: guideText
        }
    }
}


