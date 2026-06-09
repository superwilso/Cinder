//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Item
{
    property var animationImage

    property string currentImage:""
    readonly property string imagePath:"qrc:/assets/images/"
    width: icon.width
    onAnimationImageChanged: {
      if(animationImage.length > 0) {
        currentImage =  imagePath + animationImage
      } else {
        currentImage = ""
      }
    }

    Image{
        id:icon
        anchors.verticalCenter: parent.verticalCenter
        fillMode: Image.PreserveAspectFit
        source:currentImage
    }

}
