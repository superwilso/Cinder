//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item {
    id: dialogRoot

    property bool okButtonVisible: false
    // シグナル
    signal confirmed()

    QtObject {
        id: p
        property int upper_lower_radius : 4     // 上下丸みエリア用高さ
        property int list_width         : 361
        property int border_margin      : 1

        property url popup_area_center: "qrc:/assets/images/popup_area_center_4r.png"
    }

    // タイムアウトタイマ
    Timer {
        id: timeOutTimer
        interval: dmpDialog.timeout_interval // 0の時はタイマーが走らない
        running: true
        onTriggered: okButtonVisible = true
    }
    // BlockingOkダイアログ
    Rectangle {
        id: dialog

        color: "#00000000"      // 透過指定
        width: p.list_width
        height: {
            if (topText.visible && okButtonVisible)
            {
                topText.height + bottomText.height + okButton.height
            }
            else if (!topText.visible && okButtonVisible)
            {
                bottomText.height + okButton.height + p.border_margin
            }
            else if (topText.visible && !okButtonVisible)
            {
                topText.height + bottomText.height + p.border_margin
            }
            else
            {
                bottomText.height + (p.border_margin*2)
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
            visible: false
        }
        // 下部テキスト
        CenterText{
            id: bottomText
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
            iconPath: "qrc:/assets/images/btn_popup_1button_normal.png"
            pressedIconPath: "qrc:/assets/images/btn_popup_1button_pressed.png"

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
