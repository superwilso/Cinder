//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Item {
    width: 480; height: 120
    Text {
        text: "Catch the Wave"
        color: "white"
        font.pixelSize: 28
        font.family: viewstyle.font_family
        anchors {
            top: parent.top
            topMargin: 5
            horizontalCenter: parent.horizontalCenter
        }
    }

    Image {
        anchors {
            centerIn: parent
            horizontalCenterOffset: -100
            verticalCenterOffset: 18
        }
        source: "file:/opt2/appresouce/operationprev.png"
    }
    Image {
        anchors {
            centerIn: parent
            verticalCenterOffset: 18
        }
        source: "file:/opt2/appresouce/operationstop.png"
    }
    Image {
        anchors {
            centerIn: parent
            horizontalCenterOffset: 100
            verticalCenterOffset: 18
        }
        source: "file:/opt2/appresouce/operationnext.png"
    }
}
