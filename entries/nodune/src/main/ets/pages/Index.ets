import { common } from '@kit.AbilityKit';
import { AppItem, APPS } from '../catalog';

interface SuiteMeta {
  id: string;
  upstream: string;
  tagline: string;
  blurb: string;
  color: string;
  inkDark: string;
  icon: Resource;
}

const DEFAULT_META: SuiteMeta = {
  id: 'default',
  upstream: 'ArtCraft',
  tagline: 'Creative suite · 造隅创意套件',
  blurb: 'Native OpenHarmony creative application powered by Rust and wgpu.',
  color: '#2D81FF',
  inkDark: '#74AAFF',
  icon: $r('app.media.app_icon')
};

const SUITE_META: SuiteMeta[] = [
  {
    id: 'pixrune',
    upstream: 'PhotoCraft',
    tagline: 'Image editing · 图像编辑与修图',
    blurb: 'Layers, masks, non-destructive adjustments, retouching and RAW photo workflow.',
    color: '#2F7BF5',
    inkDark: '#7FB0FF',
    icon: $r('app.media.pixrune_icon')
  },
  {
    id: 'vecrune',
    upstream: 'VectorCraft',
    tagline: 'Vector illustration · 矢量插画与图形',
    blurb: 'Pen and bezier tools, boolean path operations, multi-artboard layout and crisp SVG/PDF export.',
    color: '#E8573F',
    inkDark: '#FF8A70',
    icon: $r('app.media.vecrune_icon')
  },
  {
    id: 'cliprune',
    upstream: 'FilmCraft',
    tagline: 'Video editing, color & sound · 视频剪辑与调色',
    blurb: 'Multi-track timeline editing, node/wheel color grading, keyframe effects and audio mixing.',
    color: '#8B5CF6',
    inkDark: '#B69CFF',
    icon: $r('app.media.cliprune_icon')
  },
  {
    id: 'lutrune',
    upstream: 'LightCraft',
    tagline: 'Photo library & raw develop · 照片图库与显影',
    blurb: 'Catalog organization, star/color ratings, non-destructive RAW development and film presets.',
    color: '#F2A516',
    inkDark: '#FFC14D',
    icon: $r('app.media.lutrune_icon')
  },
  {
    id: 'pagrune',
    upstream: 'PDFCraft',
    tagline: 'PDF reading & markup · PDF 阅读、整理与批注',
    blurb: 'Fast PDF viewing, highlights and annotations, page organization, signing and redaction.',
    color: '#E8204F',
    inkDark: '#F23A62',
    icon: $r('app.media.pagrune_icon')
  },
  {
    id: 'motrune',
    upstream: 'EffectCraft',
    tagline: 'Motion graphics & VFX · 动态图形与视觉特效',
    blurb: 'Keyframe graph animation, procedural effects, 2D/3D layer compositing and motion design.',
    color: '#E0368F',
    inkDark: '#FF7AB8',
    icon: $r('app.media.motrune_icon')
  },
  {
    id: 'layrune',
    upstream: 'DesignCraft',
    tagline: 'Page layout & publishing · 页面排版与出版设计',
    blurb: 'Master pages, baseline grids, linked text frames, typography styles and preflight print export.',
    color: '#7BB51C',
    inkDark: '#A6E04A',
    icon: $r('app.media.layrune_icon')
  },
  {
    id: 'framune',
    upstream: 'FigCraft',
    tagline: 'Interface design & prototyping · 界面设计与原型',
    blurb: 'Auto-layout frames, reusable components, design tokens, interactive flows and vector canvas.',
    color: '#02DA96',
    inkDark: '#4FE8B6',
    icon: $r('app.media.framune_icon')
  },
  {
    id: 'texune',
    upstream: 'TextureCraft',
    tagline: '3D texture painting · 3D 纹理绘制与烘焙',
    blurb: 'Real-time PBR viewport painting, smart materials, procedural masks and multi-channel map export.',
    color: '#C2672B',
    inkDark: '#E58A4E',
    icon: $r('app.media.texune_icon')
  },
  {
    id: 'shadrune',
    upstream: 'MaterialCraft',
    tagline: 'Procedural material authoring · 程序化材质节点',
    blurb: 'Node-based procedural noises, patterns and filters compiling live into PBR texture maps.',
    color: '#58A83E',
    inkDark: '#7CD662',
    icon: $r('app.media.shadrune_icon')
  },
  {
    id: 'quilrune',
    upstream: 'WordCraft',
    tagline: 'Writing & document design · 文档写作与版式',
    blurb: 'Structured long-form writing, styles, tables, footnotes, track changes and print-ready pages.',
    color: '#3F5FE0',
    inkDark: '#8EA2FF',
    icon: $r('app.media.quilrune_icon')
  },
  {
    id: 'sonune',
    upstream: 'SoundCraft',
    tagline: 'Audio recording & mixing · 音频录制、剪辑与混音',
    blurb: 'Waveform and spectral editing, multi-track arrangement, real-time EQ/dynamics and mastering.',
    color: '#1FA5C9',
    inkDark: '#5FD0EE',
    icon: $r('app.media.sonune_icon')
  },
  {
    id: 'deckune',
    upstream: 'DeckCraft',
    tagline: 'Presentations & slide shows · 演示文稿与幻灯片',
    blurb: 'Slide masters, built-in themes, speaker notes, vector diagrams and smooth transitions.',
    color: '#F07A1E',
    inkDark: '#FFA060',
    icon: $r('app.media.deckune_icon')
  },
  {
    id: 'dimrune',
    upstream: 'CADCraft',
    tagline: 'CAD drafting & engineering · CAD 制图与工程设计',
    blurb: 'Precision 2D/3D drafting, object snaps, layers, dimensions, command line and DWG/DXF support.',
    color: '#3EA6E8',
    inkDark: '#7CC8FF',
    icon: $r('app.media.dimrune_icon')
  },
  {
    id: 'gridrune',
    upstream: 'GridCraft',
    tagline: 'Spreadsheets & data analysis · 电子表格与数据',
    blurb: 'Fast formula engine, pivot tables, charts, conditional formatting and XLSX/CSV workbooks.',
    color: '#1F9D55',
    inkDark: '#4FD68A',
    icon: $r('app.media.gridrune_icon')
  },
  {
    id: 'modune',
    upstream: 'Craft Anything',
    tagline: 'AI-assisted studio · 智能辅助创作与插件构建',
    blurb: 'Multi-modal creative canvas with extensible plugin tools, procedural generators and automation.',
    color: '#2B90D9',
    inkDark: '#74C7EC',
    icon: $r('app.media.modune_icon')
  },
  {
    id: 'visrune',
    upstream: 'ArtCraft Studio',
    tagline: 'Generative visual studio · 生成式视觉创作工坊',
    blurb: 'Infinite node and layer canvas for visual ideation, style exploration and compositing.',
    color: '#9D50FF',
    inkDark: '#D49BFF',
    icon: $r('app.media.visrune_icon')
  }
];

@Entry
@Component
struct Index {
  @State message: string = '';
  @State query: string = '';
  @State viewMode: 'grid' | 'table' = 'grid';

  private metaFor(id: string): SuiteMeta {
    return SUITE_META.find((item: SuiteMeta) => item.id === id) ?? DEFAULT_META;
  }

  private editorApps(): AppItem[] {
    const q = this.query.trim().toLowerCase();
    return APPS.filter((app: AppItem) => {
      if (app.id === 'nodune') {
        return false;
      }
      if (q.length === 0) {
        return true;
      }
      const meta = this.metaFor(app.id);
      return (
        app.english.toLowerCase().includes(q) ||
        app.chinese.toLowerCase().includes(q) ||
        app.id.toLowerCase().includes(q) ||
        meta.upstream.toLowerCase().includes(q) ||
        meta.tagline.toLowerCase().includes(q) ||
        meta.blurb.toLowerCase().includes(q)
      );
    });
  }

  private openEditor(app: AppItem): void {
    this.message = '';
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
    context.startAbility({
      bundleName: 'com.richerfu.' + app.id,
      moduleName: app.id,
      abilityName: 'EntryAbility'
    }).catch(() => {
      this.message = '暂时无法打开 ' + app.english + ' (' + app.chinese + ')，请确认已安装该应用。';
    });
  }

  @Builder
  private TopHeader() {
    Row({ space: 12 }) {
      Image($r('app.media.nodune_icon'))
        .width(30)
        .height(30)
        .borderRadius(7)
      Text('Nodune')
        .fontSize(18)
        .fontWeight(FontWeight.Bold)
        .fontColor('#FFFFFF')
      Text('造隅工坊')
        .fontSize(12)
        .fontWeight(FontWeight.Medium)
        .fontColor('#74AAFF')
        .padding({ left: 8, right: 8, top: 3, bottom: 3 })
        .backgroundColor('#1A2436')
        .borderRadius(5)
      Text('17 Creative Apps · v0.21.0')
        .fontSize(12)
        .fontColor('#8E929B')

      Blank()

      TextInput({ placeholder: '搜索应用 (名称 / 功能 / 原型)…', text: this.query })
        .placeholderColor('#686B73')
        .fontColor('#F2F1EE')
        .fontSize(13)
        .width(260)
        .height(34)
        .padding({ left: 12, right: 12 })
        .backgroundColor('#060607')
        .borderRadius(7)
        .border({ width: 1, color: '#2A2C32' })
        .onChange((value: string) => {
          this.query = value;
        })

      Row({ space: 2 }) {
        Text('网格')
          .fontSize(12)
          .fontWeight(this.viewMode === 'grid' ? FontWeight.Medium : FontWeight.Regular)
          .fontColor(this.viewMode === 'grid' ? '#FFFFFF' : '#8E929B')
          .padding({ left: 10, right: 10, top: 6, bottom: 6 })
          .backgroundColor(this.viewMode === 'grid' ? '#23262E' : Color.Transparent)
          .borderRadius(5)
          .onClick(() => {
            this.viewMode = 'grid';
          })
        Text('列表')
          .fontSize(12)
          .fontWeight(this.viewMode === 'table' ? FontWeight.Medium : FontWeight.Regular)
          .fontColor(this.viewMode === 'table' ? '#FFFFFF' : '#8E929B')
          .padding({ left: 10, right: 10, top: 6, bottom: 6 })
          .backgroundColor(this.viewMode === 'table' ? '#23262E' : Color.Transparent)
          .borderRadius(5)
          .onClick(() => {
            this.viewMode = 'table';
          })
      }
      .padding(2)
      .backgroundColor('#0A0B0D')
      .borderRadius(7)
      .border({ width: 1, color: '#26282E' })
    }
    .width('100%')
    .height(56)
    .padding({ left: 24, right: 24 })
    .backgroundColor('#121316')
    .border({ width: { bottom: 1 }, color: '#26282E' })
  }

  @Builder
  private AppCard(app: AppItem) {
    Column() {
      // Top signature color bar
      Row()
        .width('100%')
        .height(3)
        .backgroundColor(this.metaFor(app.id).color)

      // Top hairline metadata strip (craft-launcher card_parts::top_strip)
      Row() {
        Text('v0.21.0')
          .fontSize(12)
          .fontWeight(FontWeight.Bold)
          .fontColor(this.metaFor(app.id).inkDark)
        Text(' · ' + this.metaFor(app.id).upstream)
          .fontSize(11)
          .fontColor('#7A7E87')
        Blank()
        Text(app.chinese)
          .fontSize(12)
          .fontWeight(FontWeight.Medium)
          .fontColor('#B8BBC2')
      }
      .width('100%')
      .height(34)
      .padding({ left: 16, right: 16 })
      .backgroundColor('#141519')
      .border({ width: { bottom: 1 }, color: '#23252C' })

      // Card body
      Column({ space: 12 }) {
        Row({ space: 14 }) {
          Image(this.metaFor(app.id).icon)
            .width(52)
            .height(52)
            .borderRadius(12)
            .border({ width: 1, color: '#2C2F37' })
          Column({ space: 4 }) {
            Row({ space: 8 }) {
              Text(app.english)
                .fontSize(20)
                .fontWeight(FontWeight.Bold)
                .fontColor('#FFFFFF')
              Text(app.available ? '已集成' : app.status)
                .fontSize(10)
                .fontWeight(FontWeight.Medium)
                .fontColor(app.available ? '#3FD6B8' : '#8E929B')
                .padding({ left: 6, right: 6, top: 2, bottom: 2 })
                .backgroundColor(app.available ? '#112A26' : '#1E2026')
                .borderRadius(4)
            }
            Text(this.metaFor(app.id).tagline)
              .fontSize(12)
              .fontColor(this.metaFor(app.id).inkDark)
              .maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .layoutWeight(1)
          .alignItems(HorizontalAlign.Start)
        }
        .width('100%')

        Text(this.metaFor(app.id).blurb)
          .fontSize(12)
          .fontColor('#9A9DA6')
          .lineHeight(18)
          .maxLines(2)
          .textOverflow({ overflow: TextOverflow.Ellipsis })
          .width('100%')

        Row() {
          Text('com.richerfu.' + app.id)
            .fontSize(11)
            .fontColor('#5E626B')
          Blank()
          if (app.available) {
            Button('打开')
              .type(ButtonType.Normal)
              .fontSize(12)
              .fontWeight(FontWeight.Bold)
              .fontColor('#0B0B0C')
              .backgroundColor('#FFFFFF')
              .height(30)
              .padding({ left: 16, right: 16 })
              .borderRadius(6)
              .onClick(() => this.openEditor(app))
          }
        }
        .width('100%')
        .padding({ top: 4 })
      }
      .width('100%')
      .padding(16)
    }
    .width('100%')
    .backgroundColor('#17181C')
    .borderRadius(12)
    .border({ width: 1, color: '#262830' })
    .clip(true)
  }

  @Builder
  private AppRow(app: AppItem) {
    Row({ space: 14 }) {
      Row()
        .width(4)
        .height(36)
        .borderRadius(2)
        .backgroundColor(this.metaFor(app.id).color)
      Image(this.metaFor(app.id).icon)
        .width(38)
        .height(38)
        .borderRadius(9)
      Column({ space: 2 }) {
        Row({ space: 8 }) {
          Text(app.english)
            .fontSize(16)
            .fontWeight(FontWeight.Bold)
            .fontColor('#FFFFFF')
          Text(app.chinese)
            .fontSize(13)
            .fontColor('#B8BBC2')
          Text(this.metaFor(app.id).upstream)
            .fontSize(11)
            .fontColor('#7A7E87')
        }
        Text(this.metaFor(app.id).tagline)
          .fontSize(12)
          .fontColor('#8E929B')
      }
      .alignItems(HorizontalAlign.Start)
      .layoutWeight(1)

      Text('v0.21.0')
        .fontSize(12)
        .fontWeight(FontWeight.Bold)
        .fontColor(this.metaFor(app.id).inkDark)
        .margin({ right: 8 })

      if (app.available) {
        Button('打开')
          .type(ButtonType.Normal)
          .fontSize(12)
          .fontWeight(FontWeight.Bold)
          .fontColor('#0B0B0C')
          .backgroundColor('#FFFFFF')
          .height(30)
          .padding({ left: 16, right: 16 })
          .borderRadius(6)
          .onClick(() => this.openEditor(app))
      }
    }
    .width('100%')
    .padding({ left: 16, right: 16, top: 12, bottom: 12 })
    .backgroundColor('#17181C')
    .borderRadius(10)
    .border({ width: 1, color: '#25272F' })
  }

  build() {
    Column() {
      this.TopHeader()

      if (this.message.length > 0) {
        Row({ space: 12 }) {
          Text(this.message)
            .fontSize(13)
            .fontColor('#FF9EA6')
            .layoutWeight(1)
          Text('关闭')
            .fontSize(12)
            .fontColor('#FFFFFF')
            .onClick(() => {
              this.message = '';
            })
        }
        .width('100%')
        .padding({ left: 24, right: 24, top: 10, bottom: 10 })
        .backgroundColor('#281419')
        .border({ width: { bottom: 1 }, color: '#5C232C' })
      }

      Scroll() {
        Column({ space: 16 }) {
          if (this.viewMode === 'grid') {
            GridRow({
              columns: { sm: 1, md: 2, lg: 3 },
              gutter: { x: 16, y: 16 }
            }) {
              ForEach(this.editorApps(), (app: AppItem) => {
                GridCol() {
                  this.AppCard(app)
                }
              }, (app: AppItem) => app.id)
            }
            .width('100%')
          } else {
            Column({ space: 8 }) {
              ForEach(this.editorApps(), (app: AppItem) => {
                this.AppRow(app)
              }, (app: AppItem) => app.id)
            }
            .width('100%')
          }
        }
        .width('100%')
        .padding({ left: 24, right: 24, top: 20, bottom: 28 })
      }
      .layoutWeight(1)
      .width('100%')
      .scrollBar(BarState.Auto)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#121316')
  }
}
