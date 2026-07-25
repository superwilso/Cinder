//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
    id:usbMscScreen

    property bool enableMscOperation
    property bool unmountExported
    /** type:bool USB充電対応かどうか */
    property bool has_charge_from_usb

    signal enableExportAsMsc(bool enable)

    color: "black"

    MouseArea {
        anchors.fill: parent
    }

    ScreenTitleArea {
        id: title
        z: 11
        titleText: qsTr("200016") + viewstyle.empty_string // USBマスストレージ
        visibleMusicLibraryTopicon: false
    }

    Item{
        id:usbMscItem
        anchors {top: title.bottom; left: parent.left; leftMargin: 20; right: parent.right; rightMargin: 20}

        height: usbMscIcon.height + usbMscStatusText.height
                + usbMscDescription.anchors.topMargin + usbMscDescription.height

        Image {
            id: usbMscIcon
            anchors {top: usbMscItem.top; horizontalCenter: parent.horizontalCenter}
            height: 168
            source: unmountExported === true ? "qrc:/ic_usb_connecting_msc_on.png" : "qrc:/ic_usb_connecting_msc_off.png";
        }

        SCmnLabel {
            id: usbMscStatusText
            anchors {top: usbMscIcon.bottom}
            width: parent.width
            height: 32
            font.underline: false
            horizontalAlignment: Text.AlignHCenter
            color:viewstyle.textcolor.L1
//            font.pixelSize: viewstyle.textsize.LL // viewstyle.textsize.LLは36、レイアウト指示は32
            font.pixelSize: 32
            font.weight: Font.Light
            text: ((unmountExported === true) ? qsTr("200031") : qsTr("200029")) + viewstyle.empty_string // "USB接続" : "USBマスストレージ使用中"
        }

        SCmnLabel {
            id: usbMscDescription
            anchors {top: usbMscStatusText.bottom; topMargin: 24; left: parent.left}
            width:440
            height: 288
            horizontalAlignment: Text.AlignLeft
            wrapMode: Text.WordWrap
            color: viewstyle.textcolor.L2
            font.pixelSize: viewstyle.textsize.S
            font.weight: Font.Normal
            lineHeightMode: Text.FixedHeight
            lineHeight: 36
            text: {
              if (has_charge_from_usb === true)
              {
                if (unmountExported === true)
                {
                  // "パソコンからファイルの転送や削除ができます。\n充電しながら本機の操作をするには\nUSBマスストレージをオフにしてください。"
                  qsTr("200032") + viewstyle.empty_string
                }
                else
                {
                  // "充電しながら本機の操作ができます。\nパソコンからファイルの転送や削除を行うには、\nUSBマスストレージをオンにしてください。"
                  qsTr("200030") + viewstyle.empty_string
                }
              }
              else
              {
                if (unmountExported === true)
                {
                  // "ファイルの転送中はUSB接続を解除しないでください。"
                  qsTr("200050") + viewstyle.empty_string
                }
                else
                {
                  // "パソコンからファイルの転送や削除を行うには、\nUSBマスストレージをオンにしてください。"
                  qsTr("200049") + viewstyle.empty_string
                }
              }
            }
        }

        DialogWrapper {
            id: dialog
            visible: false
            onAccepted:{
                enableExportAsMsc(true)
            }
        }
    }

    GeneralSimpleButton {
        id: usbMscButton
        anchors {top: usbMscItem.bottom; horizontalCenter: usbMscItem.horizontalCenter}
        height: 62
        enable: enableMscOperation
        text: ((unmountExported === true) ? qsTr("200033") : qsTr("200018")) + viewstyle.empty_string  // "USBマスストレージをオフにする" : "USBマスストレージをオンにする"
        text_size: viewstyle.textsize.S
        text_weight: Font.Normal
        textLeftMargin: 24  // 指定無しのため、標準でMarginは24
        textRightMargin: 24 // 指定無しのため、標準でMarginは24
        minimum_width: 200  // 指定無しのため、標準でボタン最小サイズは200px
        maximum_width: 440  // 指定無しのため、標準でボタン最大サイズはテキスト最大幅(392px)+LabelからのMargin(24)*2
        onClicked: {
            if(unmountExported === false) {
                dialog.dialogOpen("kMid0014", "", "", 0)
            } else {
                enableExportAsMsc(false)
            }
        }
    }

}

