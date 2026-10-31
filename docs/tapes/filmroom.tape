Output docs/assets/matchday-filmroom.gif
Set FontSize 16
Set Width 1000
Set Height 430
Set Shell bash
Set CursorBlink false
Set TypingSpeed 6ms
Type "playbook-tui --replay fixtures/matchday-demo.jsonl --frame 42 --form glance"
Enter
Sleep 2s
Type "clear && playbook-tui --replay fixtures/matchday-demo.jsonl --frame 42 --form tactical"
Enter
Sleep 3s
Type "clear && playbook-tui --replay fixtures/matchday-demo.jsonl --frame 42 --form analytical"
Enter
Sleep 4s
