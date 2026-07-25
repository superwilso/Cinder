//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

//カテゴリ項目
Column {
    id: settingCategory
    property string title

    signal edgeReleased()
    signal edgeCanceled()
    signal pressedYPosition(int posY)
    signal movedYPosition(int posY)

    width: childrenRect.width

    Rectangle {
        id: rectangle
        width: 480
        height: 84
        color: viewstyle.bgcolor.D1
        SCmnLabel {
            x: 8
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 4
            width: 464
            text: settingCategory.title
            color: viewstyle.textcolor.L1
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: viewstyle.textsize.S
            font.weight: Font.Normal
        }

        MouseArea {
            anchors.fill: parent
            // リリース時処理
            onReleased: settingCategory.edgeReleased()
            // キャンセル時処理(プレスしたまま画面外にドラッグ検知)
            onCanceled: settingCategory.edgeCanceled()
            // プレス時処理
            onPressed: settingCategory.pressedYPosition(mouse.y)
            // 上下移動時処理
            onMouseYChanged: settingCategory.movedYPosition(mouse.y)
        }
    }

    Image {source: "qrc:/assets/images/separator_sub.png"}
}
