//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

/**
 * @brief マーキー
 */
Item {
    id: base
    width: 332
    height: labelTitle.height
    property string fontColor: "#ffffff"
    property int    fontWeight: Font.Normal
    property bool   isUseMarquee: false
    property int    pixelSize: 16
    property string left_mask_image_path: ""
    property string right_mask_image_path: ""
    /** type:int テキストのアライメント位置 */
    property int horizontal_alignment: Text.AlignHCenter
    /** type:int テキスト表示の左側の空白幅 */
    property int left_margin: 0

    clip: true

    /**
     * 値変化してない場合でも checkMarqueeScroll() を呼び出す必要があるため
     * onXXXChanged で checkMarqueeScroll() 呼び出す作りにはできない。
     * そのため、 setter を用意し、値を変更する(可能性のある)場合は
     * checkMarqueeScroll() を必ず呼び出すようにしている。
     */
    function setMainText(text)
    {
      p.mainText = text
      checkMarqueeScroll()
    }

    QtObject {
        id: p
        /** type:string 表示文字 */
        property string mainText: ""
        property real freezeTime: 15
        property bool mainTextDisp: false
        property bool strBufJudge: false
        property int textMoveTime: 15
        onMainTextDispChanged: {
            if (mainTextDisp) {
                useMarquee()
            }
        }
    }

    //marquee animation text
    SCmnLabel {
        id: labelTitle
        x: 0
        visible: !p.mainTextDisp && isUseMarquee && p.strBufJudge
        text: p.mainText
        font.pixelSize: pixelSize
        font.weight: fontWeight
        color: fontColor
        horizontalAlignment: horizontal_alignment
        verticalAlignment: Text.AlignVCenter
        onWidthChanged: {
            p.strBufJudge = (labelTitle.width > base.width) ? true : false
            p.mainTextDisp = p.strBufJudge
        }
    }

    XAnimator {
        id: marqueeScroll
        target:labelTitle
        running: false
        from: 0;
        to: p.freezeTime - labelTitle.width - left_margin;
        duration: (labelTitle.width + left_margin) * p.textMoveTime

        onStarted: {
            p.mainTextDisp = false
        }
        onStopped: {
            p.mainTextDisp = true
        }
    }

    //hold and abridgement
    SCmnLabel {
        id: periodlabelTitle
        width: base.width
        visible: p.mainTextDisp && ( p.strBufJudge || !isUseMarquee )
        text: p.mainText
        font.pixelSize: pixelSize
        font.weight: fontWeight
        color: fontColor
        horizontalAlignment: Text.AlignLeft
        verticalAlignment: Text.AlignVCenter
    }

    //text in center and hold
    SCmnLabel {
        id: centerText
        width: base.width
        visible: !p.strBufJudge
        text: p.mainText
        font.pixelSize: pixelSize
        font.weight: fontWeight
        color: fontColor
        horizontalAlignment: horizontal_alignment
        verticalAlignment: Text.AlignVCenter
    }

    Timer {
        id: fixingTime
        interval: 2000
        running: false
        onTriggered: {
            marqueeScroll.start()
        }
    }

    function useMarquee() {
        fixingTime.running = isUseMarquee ? p.strBufJudge : false
    }

    function checkMarqueeScroll() {
        if (isUseMarquee && p.mainText !== "") {
            marqueeScroll.stop()
            fixingTime.stop()
            p.strBufJudge = (labelTitle.width > base.width) ? true : false
            p.mainTextDisp = p.strBufJudge
            labelTitle.x = 0
            useMarquee()
        } else if (fixingTime.running || marqueeScroll.running) {
            marqueeScroll.stop()
            fixingTime.stop()
        }
    }

    onIsUseMarqueeChanged: {
        checkMarqueeScroll()
    }

    //左右の消し込み用画像
    Image
    {
      visible: p.strBufJudge && marqueeScroll.running
      source: left_mask_image_path
      anchors { left: parent.left; leftMargin: -left_margin; verticalCenter: parent.verticalCenter }
    }
    Image
    {
      visible: p.strBufJudge
      source: right_mask_image_path
      anchors { right: parent.right; verticalCenter: parent.verticalCenter }
    }
}
