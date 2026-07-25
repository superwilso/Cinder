//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"
// import "qrc:/components"

Image {
    id: animationImageList

    property variant sources: []

    property alias interval: animationTimer.interval
    property alias running: animationTimer.running
    property int count: 0

    Component.onCompleted: {
        if (sources.length != 0) {
            animationImageList.source = sources[0];
        }
    }

    Timer {
        id: animationTimer
        interval: 100
        running: true
        repeat: true
        onTriggered: {
            if (sources.length != 0) {
                count = (count === sources.length - 1 ? 0 : count + 1);
                animationImageList.source = sources[count];
            }
        }
    }
}
