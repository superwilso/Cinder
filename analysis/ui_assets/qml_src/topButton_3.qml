//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0

Item {

    Loader {
        anchors.fill: parent

        width: parent.width
        height: parent.height

        source: "qrc:/SwipeGrid.qml"
    }

    Rectangle
    {
        id: topButton
        color: "grey"
        width: 150; height: 75

        anchors.left: parent.left
        anchors.bottom: parent.bottom

        MouseArea
        {
            anchors.fill: parent
        }

        Text {
            id: topButtonLabel
            anchors.centerIn: parent
            text: "On Window"
        }
    }
}

