//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Item {
    visible: true

    anchors.fill: parent

    signal updateT()

    property string title

    Connections {
        target: controller
        onAvailable:
        {
            console.log("onAvailable:", view_model)
        }
    }
    Text
    {
        text: "Genre List"
        anchors.centerIn: parent
    }
    Rectangle
    {
        id: topButton
        color: "grey"
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
