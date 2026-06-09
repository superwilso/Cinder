//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Item {
    id: base
    property real value: maximumValue * handle.x / background.width
    property color borderColor: "#646464"
    property color knobColor: "#646464"
    property real avlsValue: 50
    property real maximumValue: 0.0
    signal pressed(bool pressed)

    onValueChanged: {
        handle.x = value * background.width / maximumValue
    }

    Item {
        id: background
        width: parent.width
        height: 3
        anchors.centerIn: parent

        Rectangle {
            anchors.fill: parent
            border.color: borderColor
        }

        Rectangle {
            id: handle
            y: -height / 2
            width: 8
            height: 20
            radius: 2
            color: knobColor
            border.color: borderColor
            MouseArea {
                anchors.fill: parent
                drag.target: parent
                drag.axis: Drag.XAxis
                drag.minimumX: 0
                drag.maximumX: background.width - parent.width
                onPressedChanged: {
                    base.pressed(pressed)
                }
            }
            onXChanged: {
                var valTemp
                valTemp = maximumValue * handle.x / background.width
                var avlsX
                avlsX = avlsValue * background.width / maximumValue
                if (avlsValue<valTemp) {
                    base.value = avlsValue
                    handle.x = avlsX
                } else {
                    base.value = valTemp
                }
            }
        }
    }
}
