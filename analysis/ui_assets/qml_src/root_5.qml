//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief ダイアル用インジケーター
 * 画像への差し替えにより不要となる可能性あり
　*/
import QtQuick 2.0

Item {
    id: root
    antialiasing: true
    /** type:int 針のタイプ(0:ボリュームダイアル)/1:トーンコントロール) */
    property int type: 0

    Column {
        id: volumeDialIndicator
        Rectangle {
            width: root.width
            height: root.height * 0.8
            color: "white"
            antialiasing: true
        }
        Rectangle {
            width: root.width
            height: root.height * 0.2
            color: "transparent"
        }
    }
    Rectangle {
        id: toneControlIndicator
        width: root.width
        height: root.height
        color: "white"
    }
    states: [
        State {
            name: "volumeDial"
            when: 0 == type
            PropertyChanges { target: volumeDialIndicator; visible: true; }
            PropertyChanges { target: toneControlIndicator; visible: false; }
        },
        State {
            name: "toneControl"
            when: 1 == type
            PropertyChanges { target: volumeDialIndicator; visible: false; }
            PropertyChanges { target: toneControlIndicator; visible: true; }
        }
    ]
}
