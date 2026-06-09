//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief スピンボックス
　*/
import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2

Item {
    id: base
    /** type ListModel スピンボックスに設定するデータ。listValueエレメントが必要。 */
    property ListModel listData: [ listValue ]
    /** type int カレントインデックス (デフォルトは0) */
    property int currentIndex: 0
    /** type variant カレント値。表示中の値。 */
    property variant currentValue: null
    /** type color 背景色 (デフォルトはblack) */
    property color color: "#000000"
    height: 120
    width: 50

    QtObject {
        id: p
        property int spinHeight: base.height
        property int spinWidth: base.width
        property color fontColor: "#ffffff"
        property real fontSize: 30

        property real angle: 50.0
        property real scale: 0.8
        property int textMargin: 20

        property real spinVelocity: 300
    }
    Component.onCompleted: {
        if (listData===null) {
            listData = sampleData
        }

        spinView.positionViewAtIndex(base.currentIndex, PathView.Center);
        base.currentValue = listData.get(base.currentIndex).listValue
    }
    Rectangle {
        anchors.fill: parent
        color:base.color
        PathView {
            id: spinView
            anchors.fill: parent
            clip: true
            delegate: delegate
            model: listData
            path: Path {
                startX: base.width / 2
                startY: 0

                PathAttribute { name: "itemZ"; value: 0 }
                PathAttribute { name: "itemAngle"; value: p.angle; }
                PathAttribute { name: "itemScale"; value: p.scale; }
                PathLine { x: base.width/2; y: base.height*0.4; }
                PathPercent { value: 0.48; }
                PathLine { x: base.width/2; y: base.height*0.5; }
                PathAttribute { name: "itemAngle"; value: 0.0; }
                PathAttribute { name: "itemScale"; value: 1.0; }
                PathAttribute { name: "itemZ"; value: 100 }
                PathLine { x: base.width/2; y: base.height*0.6; }
                PathPercent { value: 0.52; }
                PathLine { x: base.width/2; y: base.height; }
                PathAttribute { name: "itemAngle"; value: -1 * p.angle; }
                PathAttribute { name: "itemScale"; value: p.scale; }
                PathAttribute { name: "itemZ"; value: 0 }
            }
            maximumFlickVelocity: p.spinVelocity
            pathItemCount: 3
            preferredHighlightBegin: 0.5
            preferredHighlightEnd: 0.5
            onMovementEnded: {
                base.currentIndex = spinView.currentIndex
                base.currentValue = listData.get(base.currentIndex).listValue
            }
        }
    }
    Rectangle {
        anchors { fill: base; }
        gradient: Gradient {
            GradientStop {position: 0.0; color: base.color}
            GradientStop {position: 0.5; color: "transparent"}
            GradientStop {position: 1.0; color: base.color}
        }
    }
    Component {
        id: delegate

        Rectangle {
            id: wrapper
            height: Math.floor(p.spinHeight / spinView.pathItemCount)
            width: p.spinWidth

            antialiasing: true
            color: base.color

            visible: PathView.onPath

            scale: PathView.itemScale
            z: PathView.itemZ

            property variant rotX: PathView.itemAngle
            transform: Rotation {
                axis { x: 1; y: 0; z: 0 }
                angle: wrapper.rotX;
                origin { x: height/3; y: height/3; }
            }
            SCmnLabel {
                anchors { fill: parent; leftMargin: p.textMargin; rightMargin: p.textMargin; }
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                text: listValue
                font.pointSize: p.fontSize
                font.bold: true
                color: p.fontColor
            }
        }
    }
    ListModel {
        id: sampleData
        ListElement { listValue: 0 }
        ListElement { listValue: 1 }
        ListElement { listValue: 2 }
        ListElement { listValue: 3 }
        ListElement { listValue: 4 }
    }
}
