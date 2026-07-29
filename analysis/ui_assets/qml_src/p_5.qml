//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item
{
    /** paramにはQVariantListがバインドされる。データの位置、種類はStatusBarListModelのenum VolumeDataPosに従う */
    property var param

    QtObject {
        id:p
        /** ハイゲインON/OFF */
        property bool isHighGain : false
        /** ボリューム値 */
        property int  volume : 0
    }

    onParamChanged: {
        p.volume = param[1]
        p.isHighGain = param[2]
    }

    width:area.width
    height:area.height

    Item{
        id:area
        width:icon.width + volumeNum.width
        height:icon.height
        anchors.verticalCenter:parent.verticalCenter
        anchors.left:parent.left
        // ボリュームアイコン画像
        Image{
            anchors.left:parent.left
            id:icon
            source: p.isHighGain ? "qrc:/assets/images/ic_volumebar_input_balance_high.png" :
                                   "qrc:/assets/images/stat_notify_volume.png"

        }
        // ボリューム数値表示
        Item{
            id:volumeNum
            anchors.left:icon.right
            anchors.top:parent.top
            anchors.bottom:parent.bottom
            width:value_text.width
            SCmnMonospaceLabel {
                id:value_text
                anchors.verticalCenter: parent.verticalCenter
                verticalAlignment: Text.AlignVCenter
                width:40
                anchors.left:parent.left
                text:("000"+p.volume).slice(-3)
                font.pixelSize: viewstyle.textsize.SS
            }
        }
    }
}
