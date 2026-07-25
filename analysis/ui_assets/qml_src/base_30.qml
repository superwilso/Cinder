//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief 複数行ラベル
 * 複数行用のラベルコンポーネント
 * wradMode及びlineHeightModeの設定を固定化。
 * テキスト長により、自動的にHeightが変更されます。
 * 使用側で指定したサイズが上書きされる可能性があります。
 */
import QtQuick 2.3
import "qrc:/parts"
Item {
  id: base
  /** type string 表示するテキスト */
  property alias text: multi_line_label.text
  /** type real 1行あたりの高さ */
  property alias lineHeight: multi_line_label.lineHeight
  /** type int 水平方向の位置情報 */
  property alias horizontalAlignment: multi_line_label.horizontalAlignment
  /** type int 垂直方向の位置情報 */
  property alias verticalAlignment: multi_line_label.verticalAlignment
  /** type int 文字PixcelSize */
  property alias pixelSize: multi_line_label.font.pixelSize
  /** type color テキストカラー */
  property alias color: multi_line_label.color
  /** type int 文字weight */
  property alias weight: multi_line_label.font.weight

  height: multi_line_label.contentHeight
  SCmnLabel {
    id: multi_line_label
    anchors.fill: parent
    font.pixelSize: 26
    font.family: viewstyle.font_family
    color: viewstyle.textcolor.L1
    wrapMode: Text.Wrap
    lineHeightMode: Text.FixedHeight
    lineHeight: base.lineHeight
    height: multi_line_label.contentHeight
  }
}
