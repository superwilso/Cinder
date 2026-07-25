//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

//通常設定項目
Item {
    id: base
    property int normalHeight: 84
    property int threeLineHeight: 126
    property int fourLineHeight: 168
    property int fiveLineHeight: 210
    property int titleLineHeight: 33
    property int labelWidth: 440


    property string title
    property string description
    property bool descriptionVisible : !(description === "")
    property bool useFixedFontSet: false
    /** enabled プロパティの代わり。マウスイベントを拾う必要があるため enabled プロパティを直接操作操作せずにこちらを操作する */
    property bool enable: true
    height: body.height
    width: body.width
    signal clicked()
    signal edgeReleased()
    signal edgeCanceled()
    signal pressedYPosition(int posY)
    signal movedYPosition(int posY)

    function getBodyHeight() {
        var bodyheight
        if (descriptionVisible)
        {
            switch(descriptionText.lineCount + titleText.lineCount)
            {
            case 2:
                bodyheight = normalHeight
                break;
            case 3:
                bodyheight = threeLineHeight;
                break;
            case 4:
                bodyheight = fourLineHeight;
                break;
            case 5:
                bodyheight = fiveLineHeight;
                break;
            default:
                bodyheight = normalHeight;
                break;
            }
        }
        else
        {
             bodyheight = normalHeight
        }
        return bodyheight
    }

    SCmnIcon {
        id: body
        width: 480
        height: getBodyHeight()
        iconPath: "qrc:/assets/images/setting_top/list_full_normal.png"
        pressedIconPath: "qrc:/assets/images/setting_top/list_full_pressed.png"
        disabledIconPath: "qrc:/assets/images/setting_top/list_full_normal.png"
        onClicked: {
            base.clicked()
        }
        onEdgeReleased: base.edgeReleased()
        // キャンセル時処理(プレスしたまま画面外にドラッグ検知)
        onEdgeCanceled: base.edgeCanceled()
        // プレス時処理
        onPressedYPosition: base.pressedYPosition(posY)
        // 上下移動時処理
        onMovedYPosition: base.movedYPosition(posY)
        Column {
            x: 20
            y: descriptionVisible
               ? Math.floor((body.height - (titleText.height + descriptionText.height + textSpacing.height)) / 2)
               : Math.floor((body.height - titleText.height) / 2)
            SCmnLabel {
                id: titleText
                width: labelWidth
                color: (enable && enabled) ? viewstyle.textcolor.L1 : viewstyle.textcolor.L3
                verticalAlignment: Text.AlignVCenter
                text: title
                font.pixelSize: viewstyle.textsize.L
                font.weight: Font.Light
                lineHeight: titleLineHeight
                lineHeightMode: Text.FixedHeight
            }
            Rectangle {
                id: textSpacing
                width: titleText.width
                height: 6
                color: "transparent"
                visible: descriptionVisible && (descriptionText.lineCount > 1 || titleText.lineCount > 1)
            }
            SCmnLabel {
                id: descriptionText
                width: labelWidth
                text: description
                color: (enable && enabled) ? viewstyle.textcolor.L2 : viewstyle.textcolor.L3
                verticalAlignment: Text.AlignVCenter
                visible: descriptionVisible
                font.pixelSize: viewstyle.textsize.S
                font.weight: Font.Light
                font.family: useFixedFontSet ? "SST_Fixed" : viewstyle.font_family
            }
        }
    }

    Image {
        anchors.bottom: base.bottom
        source: "qrc:/assets/images/separator_sub2.png"
    }

    Item {
      anchors.fill: parent
      enabled: !base.enable
      MouseArea {
        id: mouse_area
        anchors.fill: parent
        onPressed: base.pressedYPosition(mouse.y)
        onReleased: base.edgeReleased()
        onCanceled: base.edgeCanceled()
        onMouseYChanged: base.movedYPosition(mouse.y)
      }
    }
}
