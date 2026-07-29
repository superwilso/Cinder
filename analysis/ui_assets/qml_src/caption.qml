//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief ラベル
　*/
import QtQuick 2.3

Text {
    id: caption
    /** type color 通常時テキストカラー */
    property color normal_color: viewstyle.textcolor.L1
    /** type color disable時テキストカラー */
    property color disable_color: viewstyle.textcolor.L3
    font.pixelSize: viewstyle.textsize.SSS
    font.family: viewstyle.font_family
    color: enabled? normal_color : disable_color
}
