//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/"
import "qrc:/parts"

Item{
    id:contextMenu

    readonly property int titleWidth: 361   // タイトルエリア幅
    readonly property int titleHeight: 90   // タイトルエリア高さ
    readonly property int labelMargin: 20   // ラベルマージン

    signal selected(var item_id)
    signal accepted()

    anchors.centerIn: parent
    visible: true

    width: listArea.width
    height: titleArea.height + listArea.height

    QtObject {
        id: p
        property url popup_area_top: "qrc:/assets/images/popup_area_top.png"
    }

    // タイトルエリア
    Item {
        id: titleArea
        anchors {top: parent.top; left: parent.left;}
        width: contextMenu.titleWidth
        height: contextMenu.titleHeight
        Image {
            id: titleButton
            width: parent.width
            height: parent.height
            source: p.popup_area_top
        }
        SCmnLabel {
            id: titleText
            anchors {leftMargin: contextMenu.labelMargin; verticalCenter: parent.verticalCenter; left: parent.left;}
            width: parent.width - anchors.leftMargin * 2
            height: parent.height
            verticalAlignment: Text.AlignVCenter
            color: viewstyle.textcolor.L1
            font.pixelSize: viewstyle.textsize.L
            font.weight: Font.Normal
            text: dmpDialog.top_message
            maximumLineCount: 2
            elide: Text.ElideRight
            wrapMode: Text.Wrap
        }
    }

    // リストエリア
    ContextMenuList {
        id: listArea
        anchors {top: titleArea.bottom; left: titleArea.left}
        titleList: dmpDialog.list_model
        onClicked: {
            contextMenu.selected(menu_id)
            contextMenu.accepted()
        }
    }
}
