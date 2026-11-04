import { EditorAbility, editorPlugins } from '@richerfu/ability-host';

export default class EntryAbility extends EditorAbility {
  public moduleName: string = 'lutrune_ability';
  public bridgePlugins = editorPlugins('lutrune');
}
