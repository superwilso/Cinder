//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2
import "qrc:/"
import "qrc:/components"
import "qrc:/parts"

Rectangle {
    id: initialSettingWizardFinishArea
    color: viewstyle.bgcolor.D1

    signal prevScreen()
    signal nextScreen()

    SCmnLabel {
        id: mainText
        text: qsTr("010105") + viewstyle.empty_string
        width: 440
        x:20
        y: 330 - (lineHeight * lineCount / 2)
        color: viewstyle.textcolor.L1
        font.pixelSize: viewstyle.textsize.L
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        maximumLineCount: 2
        font.weight: Font.Light
        lineHeightMode: Text.FixedHeight
        lineHeight: 36
    }

    Row {
        x:28
        y: parent.height - 76
        width: 480
        spacing: 24

        SCmnIcon {
            id: prevButton
            width: 200
            height: 62
            iconPath: "qrc:/assets/images/init_wizard/btn_init_wizard_prev_normal.png"
            pressedIconPath: "qrc:/assets/images/init_wizard/btn_init_wizard_prev_pressed.png"
            disabledIconPath: "qrc:/assets/images/init_wizard/btn_init_wizard_prev_disable.png"
            onClicked: {
                prevScreen();
            }
        }

        SCmnIcon {
            id: nextButton
            width: 200
            height: 62
            iconPath: "qrc:/btn_default_normal.png"
            pressedIconPath: "qrc:/btn_default_pressed.png"
            SCmnLabel {
                width: parent.width
                height: parent.height
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pixelSize: viewstyle.textsize.S
                color:viewstyle.textcolor.L1
                text: qsTr("000000") + viewstyle.empty_string
                font.weight: Font.Light
            }
            onClicked: {
                nextScreen();
            }
        }
    }
}
