//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item {
    id: base
    width: 160
    height:childrenRect.height

    //! 公開プロパティ
    //! トーンコントロールの値
    property double value

    //! トーンコントロールの下部のラベル用テキスト
    property string text

    property alias rolling: dial.rolling

    //! signal
    signal toneChange(var value)

    signal reqSwipe()

    ToneControlValueLabel {
        id: valueLabel
        anchors {
            top: parent.top
            horizontalCenter: parent.horizontalCenter
        }
        value: base.value
    }

    SCmnDial {
        id: dial
        width: 140
        pressAndHoldDuration: 50
        anchors {
            top: valueLabel.bottom
            topMargin: 30
            horizontalCenter: parent.horizontalCenter
        }
        value: base.value
        minValue: -10
        maxValue: 10
        step: 1.0
        minRotation: -150
        maxRotation: 150
        normalDialImage: "qrc:/btn_se_setting_tone_ctrl_dial_normal.png"
        pressedDialImage: "qrc:/btn_se_setting_tone_ctrl_dial_pressed.png"
        onDialChange: {
            base.toneChange(value)
        }
        onStartedSwipe: base.reqSwipe()
    }

    SCmnLabel {
        id: title
        anchors {
            top: valueLabel.bottom
            topMargin: 184
            horizontalCenter: parent.horizontalCenter
        }
        text: base.text
        width: base.width
        height: contentHeight
        font {pixelSize: viewstyle.textsize.S; weight: Font.Light; }
        color: viewstyle.textcolor.L1
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    states: [
        State {
            when: dial.rolling
            PropertyChanges { target: valueLabel; value: dial.displayValue }
        },
        State {
            when: !dial.rolling
            PropertyChanges { target: valueLabel; value: base.value }
        }
    ]
}
