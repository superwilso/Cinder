//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0

Item {
    id: root
    property var properties
    objectName: properties["objectName"]

    height: 20

    function getIndicatorImagePath(index)
    {
        return (index === properties["now_index"])
                ? "file:/opt2/appresouce/page_indicator_focus.png"
                : "file:/opt2/appresouce/page_indicator_normal.png"
    }

    Row {
        anchors.centerIn: parent
        Repeater {
            model: properties["mark_count"]

            Image {
                width: 20; height: 20
                source: getIndicatorImagePath(index)
            }
        }
    }

    // フェード処理
    states: [
        State {
            name: "visible"
            when: properties["preview"]
            PropertyChanges { target: root; opacity: 1.0 }
        },
        State {
            name: "invisible"
            when: !properties["preview"]
            PropertyChanges { target: root; opacity: 0.0 }
        }
    ]
    transitions: [
        Transition {
            NumberAnimation { target: root; property: "opacity"; duration: root.properties["preview"] ? 0 : 200 }
        }
    ]
}
