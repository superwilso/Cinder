//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief 入力フィールド
　*/
import QtQuick 2.3

Item {
    id: base
    /**
     * 入力エリアタップ時シグナル
     * @param Nothing
     */
    signal selected();
    /** type string 表示中テキスト */
    property alias text: edit.text
    /** type int 入力可能文字数 */
    property alias maximumLength: edit.maximumLength
    /** type int 水平方向の文字寄せ　*/
    property alias horizontalAlignment: edit.horizontalAlignment
    /** type int 垂直方向の文字寄せ */
    property alias verticalAlignment: edit.verticalAlignment
    property alias inputMethodHints: edit.inputMethodHints
    property alias echoMode: edit.echoMode
    property alias inputFocus: edit.focus
    property alias inputAnchors: edit.anchors
    QtObject {
        id: p
        property color backColor: "#282828"
        property color fontColor: "#ffffff"
        property color borderColor: "#686868"
    }

    Rectangle {
        anchors.fill: parent
        color: p.backColor
        border { width: 1; color: p.borderColor; }
        TextInput {
            id: edit
            anchors.fill: parent
            font.pointSize: 20
            font.family: viewstyle.font_family
            color: p.fontColor
            onActiveFocusChanged: {
                if (activeFocus) base.selected();
            }
        }
    }
}
