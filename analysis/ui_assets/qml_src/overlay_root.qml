//
// Copyright 2015-2016 Sony Corporation
//

// 当ファイルは画面遷移FWの管理外

import QtQuick 2.0
import QtQuick.Controls 1.2
import "../components"

Item {
    id: overlay_root
    visible: true

    property var holdtoast: holdToastManager.holdToast

    // ホールド中表示
    HoldToast {
        properties: holdtoast
        anchors.bottom:parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: 72
        visible: true
    }
}
