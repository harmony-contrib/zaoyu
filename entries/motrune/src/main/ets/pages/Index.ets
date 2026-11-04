import { DefaultXComponent } from '@ohos-rs/ability';

@Entry
@Component
struct Index {
  build() {
    Stack() {
      DefaultXComponent({ moduleName: 'motrune_ability' })
    }
    .width('100%')
    .height('100%')
  }
}
