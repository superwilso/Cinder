//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2

/**
 * @brief カバーアート
 */
Item {
  id: base
  /** type url ロード中に表示する画像のパス+ファイル名 */
  property url loading_image_path: "qrc:/ic_music_library_list_loading_jacket_picture.png"
  /** type int ロード対象コンテンツのID */
  property string contents_id: ""
  /** DB更新カウンタ **/
  property var update_db_counter

  // ロード中画像
  Image {
    id: loadingImage
    visible: !coverImage.visible
    anchors.fill: parent
    source: loading_image_path
  }
  // カバーアート画像
  Image {
    id: coverImage
    visible: coverImage.progress===1.0? true : false
    height: base.height
    width: base.width
    sourceSize { height: base.height; width: base.width; }
    fillMode: Image.PreserveAspectFit
    anchors.centerIn: parent
    asynchronous: true
    source:(base.height == 0 || contents_id == "") ? "" : "image://coverart/" + contents_id + "?" + base.height + "?" + base.update_db_counter
  }
}
