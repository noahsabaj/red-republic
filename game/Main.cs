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
    private Sim.World? _world;
    private Terrain? _terrain;
    private World.Scenery? _scenery;
    private Ui.Shell? _shell;

    /// <summary>
    /// Put a building where the player clicked.
    /// </summary>
    /// <remarks>
    /// <b>The hit test asks the ground where the cursor is</b>, rather than the
    /// interface guessing from a screen position. A ray from the camera against
    /// the terrain is the only answer that stays right as the camera moves, and
    /// the refusal comes back from the simulation in words a panel can print:
    /// this file never decides whether a building may stand somewhere.
    /// </remarks>
    public override void _UnhandledInput(InputEvent @event)
    {
        if (_world is null || _shell is null || _scenery is null
            || _shell.Placing < 0
            || @event is not InputEventMouseButton click
            || !click.Pressed || click.ButtonIndex != MouseButton.Left)
        {
            return;
        }

        var at = _scenery.GroundUnder(GetViewport(), click.Position, _world.Terrain);
        if (at is null)
        {
            return;
        }

        var outcome = _world.Issue(Command.Place(_shell.Placing, at.Value.X, at.Value.Y));
        if (outcome.Accepted)
        {
            _shell.StopPlacing();
            _shell.Refused("");
        }
        else
        {
            _shell.Refused(outcome.Refusal);
        }
    }

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
        _world = Sim.World.Found(new WorldSpec(1961, 3000.0, 0), _tables);
        _terrain = _world.Terrain;
        GD.Print($"founded on a {_terrain.Cells}-cell map, "
            + $"{_terrain.FractionOf(Surface.Water) * 100.0:F1}% water");

        if (check)
        {
            GetTree().Quit();
            return;
        }

        // Where the posting actually is, which the geology decides — so it is
        // routinely nowhere near the middle of the map, and opening on the
        // centre means opening on empty ground with the town off the edge of
        // interest.
        var (atX, atZ) = Scenario.Site(_world);
        var scenery = new World.Scenery(World.Look.Default) { Name = "Scenery" };
        AddChild(scenery);
        scenery.Raise(_world, atX, atZ);
        _scenery = scenery;

        // The opening grant. A republic is a blank slate — the land, a rouble
        // balance, and whatever the player does next.
        Scenario.Found(_world);

        _shell = new Ui.Shell { Name = "Shell" };
        AddChild(_shell);
        _shell.Raise(_world);

        // `--shot` captures one frame and quits. It aims the camera itself,
        // because the sky is only visible below about twenty-five degrees and a
        // capture that cannot tilt cannot photograph it at all.
        var shot = System.Array.IndexOf(args, "--shot");
        if (shot >= 0 && shot + 1 < args.Length)
        {
            Capture(scenery, args, args[shot + 1]);
        }
    }

    /// <summary>
    /// Take one frame and quit.
    /// </summary>
    /// <remarks>
    /// <b>Never under <c>--headless</c>.</b> It waits for a frame that never
    /// arrives and spins silently for ever. And the point of it is that somebody
    /// <i>looks</i> at the image: Godot culls back faces, so geometry wound the
    /// wrong way renders as nothing at all — which reads as "not built yet" and
    /// never as "inside out", and no number in the run says otherwise.
    /// </remarks>
    private void Capture(World.Scenery scenery, string[] args, string path)
    {
        var pitch = Argument(args, "--pitch", 50.0f);
        var distance = Argument(args, "--at", 900.0f);
        scenery.Aim(pitch, distance);

        RenderingServer.FramePostDraw += () =>
        {
            var image = GetViewport().GetTexture().GetImage();
            var why = image.SavePng(path);
            GD.Print(why == Error.Ok ? $"shot {path}" : $"could not write {path}: {why}");
            GetTree().Quit(why == Error.Ok ? 0 : 1);
        };
    }

    private static float Argument(string[] args, string name, float fallback)
    {
        var at = System.Array.IndexOf(args, name);
        return at >= 0 && at + 1 < args.Length && float.TryParse(args[at + 1], out var value)
            ? value
            : fallback;
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
