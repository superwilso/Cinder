//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Item {
    visible: true

    anchors.fill: parent

    signal updateT()

    property string title
    property string test

    Connections {
        target: controller
        onAvailable:
        {
        }
        onUnavailable:
        {
        }
    }

    Text
    {
        text: title
        anchors.centerIn: parent
    }
    Text
    {
        text: test
        anchors.top: parent.top
    }
    Rectangle
    {
        id: topButton
        color: "green"
        width: 150; height: 75

        anchors.right: parent.right
        anchors.bottom: parent.bottom

        MouseArea
        {
            anchors.fill: parent

            onClicked: updateT()
        }

        Text {
            id: topButtonLabel
            anchors.centerIn: parent
            text: "On Screen"
        }
    }
}
