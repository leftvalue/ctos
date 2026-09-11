# Calibration (manual, v0.1)

`ctos` measures skill layers with a fixed L1 wrapping overhead constant
(`overhead_l1`, default 24) and, for `claude-approx`, a `chars_per_token`
divisor (default 4.0). These approximate the real client's injection cost.

## Procedure

1. Pick a real skill and note how many tokens your client reports for it
   (e.g. what Claude Code's `/skills` view shows).
2. Run:

   ```bash
   ctos calibrate --model claude path/to/skill
   ```

3. Compare `ctos`'s L1/L2 numbers with the client's figure.
4. Adjust `overhead_l1` and/or `chars_per_token` for that model in
   `config/models.toml` (or your custom `--models-config` file) until they
   line up.

## Data

Store calibration samples here (one file per model/date) so adjustments are
reproducible. Automated calibration is planned for v0.2.
