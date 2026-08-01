using Godot;

namespace RedRepublic.Ui;

/// <summary>
/// What a player has set, and where it is kept between runs.
/// </summary>
/// <remarks>
/// <para>
/// <b>"Settings that work" is a clause of the release standard</b>, and the half
/// a player notices is a value written on one run being there on the next. So
/// this reads at start-up and writes on every change rather than on exit — a
/// process that is killed rather than quit is the ordinary way a game ends, and
/// a settings file written only on a clean exit is one that loses the change
/// half the time.
/// </para>
/// <para>
/// Presentation only. Nothing here may change how a republic turns out — which
/// is why the frame rate and the display mode are here and the simulation speed
/// is not.
/// </para>
/// </remarks>
public static class Settings
{
    private const string Path = "user://settings.cfg";
    private const string Section = "player";

    public static float Volume { get; set; } = 0.7f;

    public static bool Fullscreen { get; set; }

    /// <summary>
    /// Whether to wait for the display.
    /// </summary>
    /// <remarks>
    /// Off by default during development, and that is not a preference: with it
    /// on, every frame measurement comes back as exactly the refresh interval
    /// whatever the load, so a benchmark reports a healthy number for a scene
    /// that is drowning. A player wants it on, which is why it is a setting.
    /// </remarks>
    public static bool Vsync { get; set; } = true;

    /// <summary>Read what was set last time, if anything was.</summary>
    public static void Read()
    {
        var file = new ConfigFile();
        if (file.Load(Path) != Error.Ok)
        {
            return;
        }

        Volume = (float)file.GetValue(Section, "volume", Volume);
        Fullscreen = (bool)file.GetValue(Section, "fullscreen", Fullscreen);
        Vsync = (bool)file.GetValue(Section, "vsync", Vsync);
    }

    public static Error Write()
    {
        var file = new ConfigFile();
        file.SetValue(Section, "volume", Volume);
        file.SetValue(Section, "fullscreen", Fullscreen);
        file.SetValue(Section, "vsync", Vsync);
        return file.Save(Path);
    }

    /// <summary>Make the window and the sound match what is set.</summary>
    public static void Apply()
    {
        DisplayServer.WindowSetMode(Fullscreen
            ? DisplayServer.WindowMode.Fullscreen
            : DisplayServer.WindowMode.Windowed);

        DisplayServer.WindowSetVsyncMode(Vsync
            ? DisplayServer.VSyncMode.Enabled
            : DisplayServer.VSyncMode.Disabled);

        AudioServer.SetBusVolumeDb(
            0, Volume <= 0.0f ? -80.0f : Mathf.LinearToDb(Volume));

        // A floor under the window. The instrument panels are laid out in
        // pixels — a column down each side and a hint along the bottom — and
        // below about this there is no room between them for the map. Nothing
        // enforced one, so a player could drag the window down to a strip and
        // the game had no opinion about it.
        DisplayServer.WindowSetMinSize(new Vector2I(1280, 720));
    }
}
