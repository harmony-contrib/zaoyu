import { EditorAbility, editorPlugins } from '@richerfu/ability-host';

export default class EntryAbility extends EditorAbility {
  public moduleName: string = 'visrune_ability';
  public bridgePlugins = editorPlugins('visrune');
}
