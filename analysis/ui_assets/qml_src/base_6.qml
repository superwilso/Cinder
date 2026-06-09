//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"

//スイッチボタン付き設定項目
Column {
    id: base

    property alias checked: switchButton.checked

    signal clicked()
    signal edgeReleased()
    signal pressedYPosition(int posY);
    signal movedYPosition(int posY);
    signal edgeCanceled()

    Rectangle {
        id: body
        width: 480
        height: 84
        color: viewstyle.bgcolor.D1

        SCmnSwitchButtonWithSubject {
            id: switchButton
            itemConnectable: true
            loadComponent: Item {
                anchors.fill: parent
                SCmnLabel {
                    anchors { fill: parent; leftMargin: 20; }
                    text: (switchButton.checked ? qsTr("000042") : qsTr("000043")) + viewstyle.empty_string // ON or OFF
                    verticalAlignment: Text.AlignVCenter
                    horizontalAlignment: Text.AlignLeft
                    color: viewstyle.textcolor.L1
                    font.pixelSize: viewstyle.textsize.L
                    font.weight: Font.Light
                }
            }

            onClicked: base.clicked()
            onEdgeReleased: base.edgeReleased()
            onPressedYPosition: base.pressedYPosition(posY)
            onMovedYPosition: base.movedYPosition(posY)
            onEdgeCanceled: base.edgeCanceled()
        }
    }
}
