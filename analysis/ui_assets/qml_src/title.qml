//
// Copyright 2017 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"

Rectangle {
    color: viewstyle.bgcolor.D1

    property var metaDetailedInfoList

    signal touchListAt(int index)

    ScreenTitleArea {
        id: title
        titleText: qsTr("020308") + viewstyle.empty_string
        fontWeight: Font.Light
    }

    ListView {
        id: list
        width: parent.width
        height: parent.height - title.height
        y: title.height
        model: metaDetailedInfoList
        delegate: delegate
        boundsBehavior: Flickable.StopAtBounds
        clip: true
    }

    Component {
        id: delegate
        Column{
            SCmnIcon {
                id: metaEditListIcon
                width: 480
                height: 84
                iconPath: "qrc:/assets/images/setting_top/list_full_normal.png"
                pressedIconPath: "qrc:/assets/images/setting_top/list_full_pressed.png"
                disabledIconPath: "qrc:/assets/images/setting_top/list_full_normal.png"

                property bool isPressed: false

                onClicked: {
                    touchListAt(index)
                    isPressed = false
                }
                onPressed: {
                    isPressed = true
                }
                onReleased: {
                    isPressed = false
                }
                onEdgeCanceled: {
                    isPressed = false
                }
                Column {
                    x: 20
                    y: Math.floor((parent.height - (titleText.height + nameText.height + textSpacing.height)) / 2)
                    SCmnLabel {
                        id: titleText
                        width: 440
                        color: viewstyle.textcolor.L1
                        verticalAlignment: Text.AlignVCenter
                        text: item_title
                        font.pixelSize: viewstyle.textsize.L
                        font.weight: Font.Light
                        lineHeight: 33
                        lineHeightMode: Text.FixedHeight
                    }
                    Rectangle {
                        id: textSpacing
                        width: titleText.width
                        height: 6
                        color: "transparent"
                    }
                    SCmnLabel {
                        id: nameText
                        width: 440
                        text: item_value
                        clip: true
                        color: viewstyle.textcolor.L2
                        verticalAlignment: Text.AlignVCenter
                        font.pixelSize: viewstyle.textsize.S
                        font.weight: Font.Light
                        Image {  // 消し込み画像は、表示時には必須エレメントではないため、遅延して表示させる。
                          asynchronous: true
                          anchors.right: parent.right
                          anchors.bottom: parent.bottom
                          height: parent.height
                          visible: (parent.width - 48 >= parent.implicitWidth) ? false : true
                          source: visible ? (metaEditListIcon.isPressed ?
                                                 "qrc:/ic_list_item_mask_right_pressed.png" :
                                                 "qrc:/ic_list_item_mask_right.png")
                                          : ""
                        }
                    }
                }
            }

            Image {
                source: "qrc:/assets/images/separator_sub2.png"
            }
        }
    }
}
