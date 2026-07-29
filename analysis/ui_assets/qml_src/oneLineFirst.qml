//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

/* 注意：このコンポーネントはDialog以外では使わないで下さい */

Item {
    width: parent.width
    height: 90

    visible: firstLine === "" ? false : true

    property string firstLine: {
        if ((stringSplit(0))) {
            stringSplit(0)
        } else {
            ""
        }
    }
    property string secondLine: {
        if ((stringSplit(1))) {
            stringSplit(1)
        } else {
            ""
        }
    }

    // タイトル背景
    Image {
        anchors.fill: parent
        source: "qrc:/assets/images/popup_area_top.png" // 361 x 90
    }

    // 1行タイトル：1行目
    SCmnLabel {
        id: oneLineFirst

        visible: secondLine === "" ? true : false
        anchors {
            left: parent.left; right: parent.right;
            leftMargin: 20; rightMargin: 20;
            verticalCenter: parent.verticalCenter;
        }
        wrapMode: Text.Wrap
        text: firstLine
        font.pixelSize: viewstyle.textsize.L
        font.weight: Font.Normal
        color: viewstyle.textcolor.L1
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignLeft
        maximumLineCount: 1
        elide: Text.ElideRight
    }

    // 2行タイトル
    Column {
        id: twoLinesTitle

        visible: oneLineFirst.visible === true ? false : true
        anchors.fill: parent

        // 1行目
        SCmnLabel {
            id: firstLabel

            width: parent.width
            height: parent.height * 0.5
            anchors {
                left: parent.left; right: parent.right;
                leftMargin: 20; rightMargin: 20;
            }
            wrapMode: Text.Wrap
            text: firstLine
            font.pixelSize: viewstyle.textsize.L
            font.weight: Font.Normal
            color: viewstyle.textcolor.L1
            verticalAlignment: Text.AlignBottom
            horizontalAlignment: Text.AlignLeft
            maximumLineCount: 1
            elide: Text.ElideRight
        }

        // 2行目
        SCmnLabel {
            id: secondLabel

            width: parent.width
            height: parent.height * 0.5
            anchors {
                left: parent.left; right: parent.right;
                leftMargin: 20; rightMargin: 20;
            }
            wrapMode: Text.Wrap
            text: secondLine
            font.pixelSize: viewstyle.textsize.L
            font.weight: Font.Normal
            color: viewstyle.textcolor.L2
            verticalAlignment: Text.AlignTop
            horizontalAlignment: Text.AlignLeft
            maximumLineCount: 1
            elide: Text.ElideRight
        }
    }

    // 改行分割
    function stringSplit(num)
    {
        var a = dmpDialog.top_message.split("\n")
        return a[num]
    }
}
