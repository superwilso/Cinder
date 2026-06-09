//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

/* Tab Control Element
   This component is used on tab screen.
   Default tab names are "Tone Control" and "Equalizer".
   You need other tab names, you can set tabNames property.
*/

Item {
    width: 480; height: 40
    property variant tabNames: ["Tone Control", "Equalizer"]    // tab names setting
    property variant currentTabName: tabNames[currentIndex]     // current tab name
    property int tabCount: tabNames.length                      // tab count
    property int currentIndex: 0                                // current tab index

    signal tabChanged(int index)    // When tab has changed, tabChanged signal is emitted.

    Row {
        anchors.fill: parent
        Repeater {
            model: tabCount
            Item {               // tab
                width: 480 / tabCount; height: parent.height
                Rectangle {      // rounded rectangle
                    width: parent.width * 0.8; height: parent.height * 0.9
                    color: index == currentIndex ? "#333333" : "black"
                    anchors.centerIn: parent
                    radius: height * 0.5
                    Text {       // text on rounded rectangle
                        anchors.centerIn: parent
                        color: index == currentIndex ? "white" : "#999999"
                        text: tabNames[index]
                        font.pointSize: 14
                        font.family: viewstyle.font_family
                    }
                    MouseArea {  // tab's mouse area
                        anchors.fill: parent
                        onPressed: {  // When tab has pressed, tab is changed.
                            currentIndex = index
                            tabChanged(currentIndex)
                        }
                    }
                }
            }
        }
    }
}
