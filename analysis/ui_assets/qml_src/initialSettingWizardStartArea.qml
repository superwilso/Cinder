//
// Copyright 2015-2017 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2
import "qrc:/"
import "qrc:/components"
import "qrc:/parts"
import "qrc:/sid_4401_date_setting_top"

Rectangle {
    id: initialSettingWizardStartArea
    color: viewstyle.bgcolor.D1

    property bool isAvailableLanguageSelection
    property bool isLanguageSettingTypeAll

    signal nextScreen()

    // language selection unavailable
    SCmnLabel {
        id: dateSettingLabel
        text: qsTr("010104") + viewstyle.empty_string
        width: 440
        x:20
        y: 330 - (lineHeight * lineCount / 2)
        visible: !isAvailableLanguageSelection
        color: viewstyle.textcolor.L1
        font.pixelSize: viewstyle.textsize.L
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        font.weight: Font.Light
        maximumLineCount: 2
        lineHeightMode: Text.FixedHeight
        lineHeight: 36
    }

    // language selecton available
    SCmnLabel {
        text: (isLanguageSettingTypeAll ? qsTr("010103") : qsTr("010126")) + viewstyle.empty_string
        width: 440
        x:20
        y: isLanguageSettingTypeAll ? 57 : 330 - (lineHeight * lineCount / 2)
        visible: isAvailableLanguageSelection
        color: viewstyle.textcolor.L1
        font.pixelSize: viewstyle.textsize.L
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        lineHeight: font.pixelSize + 6
        lineHeightMode: Text.FixedHeight
        font.weight: Font.Light
    }

    DisclaimerGuide {
        anchors {
            top: dateSettingLabel.bottom;
            left: parent.left;
            bottom: nextButton.top;
            right: parent.right;
        }
        visible: !isAvailableLanguageSelection
        guideText: qsTr("010124") + viewstyle.empty_string
    }

    SCmnIcon {
        id: nextButton
        x: 140
        y: parent.height - 76
        width: 200
        height: 62
        iconPath: "qrc:/assets/images/init_wizard/btn_init_wizard_next_normal.png"
        pressedIconPath: "qrc:/assets/images/init_wizard/btn_init_wizard_next_pressed.png"
        disabledIconPath: "qrc:/assets/images/init_wizard/btn_init_wizard_next_disable.png"
        onClicked: {
            nextScreen();
        }
    }
}
