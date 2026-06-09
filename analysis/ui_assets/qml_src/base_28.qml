//
// Copyright 2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item {
    id: base

    //! 公開プロパティ
    /** type:bool テキストエリア領域の高さを強制的に決定するかどうか */
    property bool is_force_text_area_height: false
    /** type string 表示するテキスト */
    property string text: ""

    QtObject {
        id: p
        property int line_height:            33
        property int list_area_height:       88
        property int force_list_area_height: 518
        property int text_area_margin:       20
    }

    width: parent.width
    height: textItemArea.height

    // テキストライン数計算に使用
    SCmnLabel {
        id: dummy

        height: childrenRect.height
        visible: false

        anchors {
            left: parent.left; right: parent.right;
            leftMargin: p.text_area_margin;
            rightMargin: p.text_area_margin;
        }
        text: base.text
        lineHeight: p.line_height
        horizontalAlignment: Text.AlignLeft
        verticalAlignment: Text.AlignVCenter
        font.pixelSize: viewstyle.textsize.L
        font.weight: Font.Light
        color: viewstyle.textcolor.L1
        wrapMode: Text.Wrap
    }
    // 下部テキスト
    Item {
        id: textItemArea

        width: parent.width
        height: {
            if(is_force_text_area_height) {
                p.force_list_area_height
            }else{
                if(dummy.lineCount <= 4){
                    p.list_area_height * 2
                }else if(dummy.lineCount <= 6){
                    p.list_area_height * 3
                }else if(dummy.lineCount <= 9){
                    p.list_area_height * 4
                }else{
                    p.list_area_height * 4
                }
            }
        }

        clip: true

        SCmnScrollIndicator {
            id: scroll
            flickable: textArea
            anchors {
                top: textArea.top; bottom: textArea.bottom; right: textItemArea.right;
                rightMargin: 8+4+1;      // 8:(右マージン)+4(バー幅)+1(外枠線分)
            }
            visible: textArea.interactive
        }


        Flickable{
            id: textArea

            anchors.fill: parent
            anchors.margins: p.text_area_margin
            contentWidth: width
            contentHeight: bottomTextLabel.height
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            clip: true
            interactive: contentHeight > height ? true : false

            Item{
                width: textArea.width
                height: textArea.height

                SCmnMultiLineLabel {
                    id: bottomTextLabel
                    width: parent.width
                    text: base.text
                    lineHeight: p.line_height
                    horizontalAlignment: Text.AlignLeft
                    anchors.verticalCenter: textArea.interactive ? undefined : parent.verticalCenter
                    verticalAlignment: textArea.interactive ? Text.AlignTop : Text.AlignVCenter

                    pixelSize: viewstyle.textsize.L
                    weight: Font.Light
                    color: viewstyle.textcolor.L1
                }
            }
        }
    }
}
