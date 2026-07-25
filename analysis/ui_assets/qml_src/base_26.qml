//
// Copyright 2015-2016 Sony Corporation
//
/**
 * @brief 可変長幅対応コンポーネント
　*/
import QtQuick 2.0

Item {
    id: base

    /** type int 最小横幅 (デフォルトは50) */
    property int min_width: 50
    /** type int 最大横幅 (デフォルトは150) */
    property int max_width: 150
    /** type string テキスト文字 (デフォルトは"sample") */
    property string text_str: "sample"
    /** type string テキスト色 (デフォルトは"white") */
    property string text_color: "white"
    /** type int ピクセルサイズ (デフォルトは18) */
    property int text_size: 18
    /** type int 文字太さ (デフォルトはNormal) */
    property int text_weight: Font.Normal
    /** type int 水平文字位置 */
    property int horizontalAlignment: Text.AlignHCenter
    /** type int 垂直文字位置 */
    property int verticalAlignment: Text.AlignVCenter
    /** type int テキストの左右空き領域 */
    property int textLeftMargin: 0
    property int textRightMargin: 0

    height: 50
    width: 120

    SCmnLabel {
        id: buttonText
        anchors { top: parent.top; left: parent.left; centerIn: parent;
                  leftMargin: base.textLeftMargin; rightMargin: base.textRightMargin}
        height: parent.height
        text: base.text_str
        color: base.text_color
        font.pixelSize: base.text_size
        font.weight: base.text_weight
        horizontalAlignment: base.horizontalAlignment
        verticalAlignment: base.verticalAlignment
        onTextChanged: {
            base.changeButtonWidth()
        }
    }

    Component.onCompleted: {
        changeButtonWidth()
    }

    function changeButtonWidth()  {
        var margin_size = 0
        if (max_width <= min_width ) {
            buttonText.width = min_width
        } else {
            margin_size = base.textLeftMargin + base.textRightMargin;
            if (buttonText.contentWidth+margin_size <= min_width) {
                buttonText.width = min_width
                margin_size = 0
            } else {
                if (buttonText.contentWidth+margin_size > max_width) {
                    buttonText.width = max_width
                    margin_size = 0
                } else {
                    buttonText.width = buttonText.contentWidth
                }
            }
        }
        base.width = buttonText.width + margin_size
    }
}
