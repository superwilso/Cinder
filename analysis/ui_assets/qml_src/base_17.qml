//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

//プルダウンリスト付き設定項目
Column {
    id: base

    property alias dialogTitle : pulldownlist.dialogTitle
    property alias listData : pulldownlist.listData
    property alias currentText : pulldownlist.currentText
    property alias pulldownEnabled : pulldownlist.enabled

    signal selected(var item_id)
    signal edgeReleased()
    signal edgeCanceled()
    signal pressedYPosition(int posY)
    signal movedYPosition(int posY)

    SCmnPulldownList {
        id: pulldownlist
        width: 440
        height: 84
        okButton: false
        pulldownHeight: 84
        onEdgeReleased: base.edgeReleased()
        onEdgeCanceled: base.edgeCanceled()
        onPressedYPosition: base.pressedYPosition(posY)
        onMovedYPosition: base.movedYPosition(posY)
    }

    Image {source: "qrc:/assets/images/separator_sub2.png"}

    Connections {
        id: connections
        ignoreUnknownSignals: true
        target: pulldownlist
        onSelected:base.selected(parseInt(item_id))
    }
}
