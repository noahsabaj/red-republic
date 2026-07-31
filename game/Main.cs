using Godot;
using RedRepublic.Sim;

namespace RedRepublic;

/// <summary>
/// The root of the running game, and the only thing that knows both halves.
/// </summary>
/// <remarks>
/// <para>
/// This is where files are opened, because <c>res://</c> is a Godot idea: in an
/// exported build the data is packed and there is no filesystem path to hand
/// anybody. So the engine side reads the bytes and the simulation is handed a
/// string — which is why <see cref="Tables.Load"/> takes text rather than a
/// path, and is the shape every other file the simulation needs will follow.
/// </para>
/// <para>
/// <c>--check</c> founds a republic, says so, and quits. It exists because both
/// alternatives are wrong for CI: a screenshot run waits for a frame that never
/// arrives under <c>--headless</c> and spins for ever, and a plain run never
/// exits — so the job would hang on a success and depend on a timeout to report
/// it.
/// </para>
/// </remarks>
public partial class Main : Node3D
{
    private Tables? _tables;
    private Terrain? _terrain;

    public override void _Ready()
    {
        var args = OS.GetCmdlineUserArgs();
        var check = System.Array.IndexOf(args, "--check") >= 0;

        // Rewrite the theme from the palette and quit. A checked-in generator
        // and a committed artifact: the editor previews what the game ships, and
        // "the hairline is one shade lighter" is one constant rather than
        // twenty-six literals in a resource nobody can read.
        if (System.Array.IndexOf(args, "--build-theme") >= 0)
        {
            var why = Ui.ThemeBuilder.Build();
            GD.Print(why == Error.Ok
                ? $"wrote {Ui.ThemeBuilder.Path}"
                : $"could not write {Ui.ThemeBuilder.Path}: {why}");
            GetTree().Quit(why == Error.Ok ? 0 : 1);
            return;
        }

        _tables = LoadTables();
        GD.Print($"tables ok: {_tables.BuildingCount} buildings, "
            + $"{_tables.Resources.Length} resources, checksum {_tables.ChecksumGot}");

        // A seed a person can type. The founding shelf will own this properly;
        // for now it is what makes `--check` exercise worldgen rather than only
        // the table.
        _terrain = Terrain.Generate(1961, 3000.0, _tables);
        GD.Print($"founded on a {_terrain.Cells}-cell map, "
            + $"{_terrain.FractionOf(Surface.Water) * 100.0:F1}% water");

        if (check)
        {
            GetTree().Quit();
        }
    }

    /// <summary>
    /// Reads the balance table through Godot, so it works from an export where
    /// the data is inside the pack rather than beside the executable.
    /// </summary>
    private static Tables LoadTables()
    {
        using var file = FileAccess.Open("res://data/manifest.json", FileAccess.ModeFlags.Read);
        if (file is null)
        {
            // Not a warning. Without the table there is no economy, no building
            // costs and no prices; a game that carried on from here would be a
            // menu in front of nothing.
            throw new System.IO.FileNotFoundException(
                $"res://data/manifest.json could not be opened: {FileAccess.GetOpenError()}");
        }

        return Tables.Load(file.GetAsText());
    }
}
