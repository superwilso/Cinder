//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0

Item {
    anchors.fill: parent

    signal back()

    property string myTitle

    Connections {
        target: windowController
        onAvailable:
        {
        }
        onUnavailable:
        {
        }
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

            onClicked: back()
        }

        Text {
            id: topButtonLabel
            anchors.centerIn: parent
            text: "Back"
        }
    }
}

