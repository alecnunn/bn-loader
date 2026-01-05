use crate::ShellType;

pub(crate) fn print_instructions(shell: &ShellType) {
    match shell {
        ShellType::Bash => {
            println!("# Add this to your ~/.bashrc:");
            println!("source <(COMPLETE=bash bn-loader)");
        }
        ShellType::Zsh => {
            println!("# Add this to your ~/.zshrc:");
            println!("source <(COMPLETE=zsh bn-loader)");
        }
        ShellType::Fish => {
            println!("# Add this to ~/.config/fish/config.fish:");
            println!("COMPLETE=fish bn-loader | source");
        }
        ShellType::Powershell => {
            println!("# Add this to your $PROFILE:");
            println!("$env:COMPLETE = 'powershell'");
            println!("bn-loader | Out-String | Invoke-Expression");
            println!("Remove-Item Env:COMPLETE");
        }
    }
}
