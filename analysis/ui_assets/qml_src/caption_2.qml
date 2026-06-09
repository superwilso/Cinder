//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief 数値のみ等幅フォントとなるフォントセットを利用したSCmnLabel
　*/
import QtQuick 2.3
import "qrc:/parts"

Text {
    id: caption
    /** type color 通常時テキストカラー */
    property color normal_color: viewstyle.textcolor.L1
    /** type color disable時テキストカラー */
    property color disable_color: viewstyle.textcolor.L3
    font.pixelSize: viewstyle.textsize.SSS
    font.family: "SST_Fixed"
    color: enabled? normal_color : disable_color
}
