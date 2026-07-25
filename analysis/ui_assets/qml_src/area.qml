//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

Item
{
    property var param

    width: parent.width
    height: parent.height

    Item{
        id: area
        anchors.fill:parent
        width: childrenRect.width
        // ボリューム変更時文字列表示
        Item{
            id: volume
            Image {
                id: volume_icon
                anchors.left: parent.left
                source: "qrc:/assets/images/stat_notify_volume.png"
            }
            Image {
                id: volume_change
                anchors.left: volume_icon.right
                source: (param === 1) ? "qrc:/assets/images/ic_volumebar_volume_plus.png" : "qrc:/assets/images/ic_volumebar_volume_minus.png"
            }
        }
    }
}
