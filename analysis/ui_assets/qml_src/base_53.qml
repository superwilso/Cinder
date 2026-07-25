//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/"

Item {
    id: base
    visible: true
    property alias title: title.text
    property double balance_value
    signal balanceValueChange(real newValue)

    Rectangle {
        id: background
        anchors.fill: parent
        color: viewstyle.bgcolor.D1
    }

    Item {
        id: titleArea
        width: 480
        height: 88
        anchors { top: base.top; }
        SCmnLabel {
            id: title
            anchors { top: parent.top; bottom: parent.bottom; left: parent.left; leftMargin: 20; right: parent.right; }
            font {pixelSize: viewstyle.textsize.L; weight: Font.Light; }
            color: viewstyle.textcolor.L1
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
        }
    }

    Image {
        id: separator
        anchors { top: titleArea.bottom; left: parent.left; }
        source: "qrc:/assets/images/separator_main.png"
    }

    SCmnMonospaceLabel {
        id: balanceValueText
        width: 260
        anchors { top: separator.bottom; topMargin: 252; horizontalCenter: parent.horizontalCenter; }
        font {pixelSize: viewstyle.textsize.M; weight: Font.Light; }// textsize.U_S
        color:viewstyle.textcolor.L1
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        text: setText() + viewstyle.empty_string
        function setText() {
            if (balance_value === 0) {
                return qsTr("020115") // CENTER
            } else if (balance_value < 0) {
                return qsTr("020114").replace("%s",(Math.abs(balance_value)).toFixed(1)) // L+  String(Math.abs(balance_value)) db
            } else {
                return qsTr("020113").replace("%s",(Math.abs(balance_value)).toFixed(1)) // R+  String(Math.abs(balance_value)) db
            }
        }
    }

    Stepper {
        id: stepper
        anchors {
            top: separator.bottom; topMargin: 261;
            left: parent.left; leftMargin: 22;
            right: parent.right; rightMargin: 22;
        }
        height: 88
        keySize: 88

        onDownValue: {
            slider.decrementValue()
        }
        onUpValue: {
            slider.incrementValue()
        }

        upKeyLoadComponent: Image {
            anchors.fill: parent
            source: "qrc:/btn_se_setting_lr_balance_right_normal.png"
        }
        upKeyPressedComponent: Image {
            anchors.fill: parent
            source: "qrc:/btn_se_setting_lr_balance_right_pressed.png"
        }
        downKeyLoadComponent: Image {
            anchors.fill: parent
            source: "qrc:/btn_se_setting_lr_balance_left_normal.png"
        }
        downKeyPressedComponent: Image {
            anchors.fill: parent
            source: "qrc:/btn_se_setting_lr_balance_left_pressed.png"
        }
    }

    SCmnSlider {
        id: slider
        anchors { verticalCenter: stepper.verticalCenter; horizontalCenter: parent.horizontalCenter; }
        width: 260
        height: 88

        minValue: -3
        maxValue: 3
        value: base.balance_value
        sliderStep: 0.5

        onValueChanged: {
            base.balanceValueChange(value)
        }

        slider_style: SliderStyle {
            groove: Image {
                anchors.centerIn: parent
                source: "qrc:/slider_se_setting_lr_balance_guide.png"
            }
            handle: Image {
                anchors.centerIn: parent
                source: "qrc:/slider_se_setting_lr_balance_pointer.png"
            }
        }
    }
    onBalance_valueChanged: {
      // Demoモード時に非画面表示時に値が変更された後、画面再表示時に正確に値反映するための処理。
      if (!slider.sliderPressed)
      {
        slider.changeValue(base.balance_value)
      }
    }
}
