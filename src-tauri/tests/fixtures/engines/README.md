# Voorbeelduitvoer van andere AI-CLI's (fase 11b)

Echt opgenomen (Codex CLI 0.172.0 en Gemini CLI 0.71.1, zonder inlog, in de testomgeving):
- `gemini_not_logged_in.txt`, de eerste twee regels van `codex_reconnect.jsonl`.

Samengesteld uit de bron en documentatie, **niet** opgenomen uit een echte sessie (er is hier geen account):
- De rest van de bestanden. De gebeurtenisnamen en velden zijn gecontroleerd: voor Gemini in de bron van de CLI (`JsonStreamEventType`: init, message, tool_use, tool_result, error, result en hun velden), voor Codex als tekenreeksen in het programma (`thread.started`, `turn.completed`, `turn.failed`, `item.completed`, `agent_message`, `command_execution`, `file_change`, `mcp_tool_call`, `cached_input_tokens`, "You've hit your usage limit"). De exacte JSON van een item (bv. `aggregated_output`) en de limiet-teksten volgen de documentatie en gebruikersmeldingen en kunnen per versie afwijken.
