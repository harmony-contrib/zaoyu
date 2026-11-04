import { EditorAbility, editorPlugins } from '@richerfu/ability-host';

export default class EntryAbility extends EditorAbility {
  public moduleName: string = 'framune_ability';
  public bridgePlugins = editorPlugins('framune');
}
