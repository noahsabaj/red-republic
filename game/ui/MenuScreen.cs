using System;
using System.Globalization;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// The menu: saving, loading, settings, and leaving.
/// </summary>
/// <remarks>
/// <para>
/// <b>One screen rather than four.</b> A pause menu that opens a settings dialog
/// that opens a save browser is three modal layers deep for a decision that is
/// one decision, and every layer of it is another Escape the player has to guess
/// at. This is a sheet with three sections on it.
/// </para>
/// <para>
/// <b>Nothing here is behind an "are you sure".</b> Saving is reversible, loading
/// is what a save is for, and a setting is a thing you change back. The one
/// irreversible action is quitting, and a republic that has been saved loses
/// nothing by it — so the sheet says when it was last saved rather than asking a
/// question.
/// </para>
/// </remarks>
public sealed partial class MenuScreen : Screen
{
    private VBoxContainer _saves = null!;
    private Label _said = null!;
    private HSlider _volume = null!;
    private Button _fullscreen = null!;
    private LineEdit _name = null!;
    private Button _vsync = null!;

    protected override string Title => "Council of Ministers";

    protected override string Blurb =>
        "Save the republic, take one up again, or set the machine down.";

    /// <summary>Raised when a save has been read, so the world can be rebuilt around it.</summary>
    public event Action<Sim.World>? Loaded;

    public override void Refresh()
    {
        if (!_name.HasFocus())
        {
            _name.Text = Republic.Name;
        }

        Listing();
        _volume.Value = Settings.Volume;
        _fullscreen.ButtonPressed = Settings.Fullscreen;
        _fullscreen.Text = Parts.Word(Settings.Fullscreen);
        _vsync.ButtonPressed = Settings.Vsync;
        _vsync.Text = Parts.Word(Settings.Vsync);
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);
        var columns = Parts.Columns(body);

        // ---- the republic ----
        var left = Parts.Section(columns, "This republic", "", 1.0f);

        // <b>The second beat of founding, and nothing offered it.</b> A name is
        // an input like any other: it is journalled, so a replayed republic
        // comes back called what the player called it. Without a control every
        // save was written as "Unnamed" and two on one day overwrote each other.
        var naming = Parts.Row(left);
        naming.AddChild(Parts.Cell(Parts.Say("Called"), 1.0f));
        _name = new LineEdit
        {
            PlaceholderText = "Novaya Zarya",
            SizeFlagsHorizontal = SizeFlags.ExpandFill,
        };

        _name.TextSubmitted += text => Ask(Command.NameRepublic(text));
        _name.FocusExited += () => Ask(Command.NameRepublic(_name.Text));
        naming.AddChild(Parts.Cell(_name, 2.0f));

        var save = Parts.Press("Save the republic", "Primary");
        save.Pressed += Save;
        left.AddChild(save);

        _said = Parts.Say("", "Faint");
        left.AddChild(_said);

        left.AddChild(Parts.Gap(Palette.GapSection));
        left.AddChild(Parts.Say("SETTINGS", "Section"));

        var sound = Parts.Row(left);
        sound.AddChild(Parts.Cell(Parts.Say("Sound"), 1.4f));
        _volume = new HSlider { MinValue = 0.0, MaxValue = 1.0, Step = 0.05 };
        _volume.ValueChanged += value =>
        {
            Settings.Volume = (float)value;
            Settings.Write();
        };

        sound.AddChild(Parts.Cell(_volume, 1.6f));

        var screen = Parts.Row(left, alt: true);
        screen.AddChild(Parts.Cell(Parts.Say("Fill the screen"), 1.4f));
        _fullscreen = Parts.Toggle();
        _fullscreen.Toggled += on =>
        {
            // The caller writes the word, because a handler that captured the
            // button to set its own label is a reference cycle the engine
            // reports as a leak at exit.
            _fullscreen.Text = Parts.Word(on);
            Settings.Fullscreen = on;
            Settings.Apply();
            Settings.Write();
        };

        screen.AddChild(Parts.Cell(_fullscreen, 1.6f));

        var sync = Parts.Row(left);
        sync.AddChild(Parts.Cell(Parts.Say("Wait for the display"), 1.4f));
        _vsync = Parts.Toggle();
        _vsync.Toggled += on =>
        {
            _vsync.Text = Parts.Word(on);
            Settings.Vsync = on;
            Settings.Apply();
            Settings.Write();
        };

        sync.AddChild(Parts.Cell(_vsync, 1.6f));

        left.AddChild(Parts.Gap(Palette.GapSection));
        var quit = Parts.Press("Set the machine down", "Quiet");
        quit.Pressed += () => GetTree().Quit();
        left.AddChild(quit);

        // ---- the saves ----
        _saves = Parts.Scroller(
            Parts.Section(columns, "Republics on file", "Newest first.", 1.2f));
    }

    private void Save()
    {
        var name = Republic.Name.Length > 0 ? Republic.Name : "Unnamed";
        var date = Republic.Clock.Date;

        // The tick as well as the date. Two saves on one day used to be one
        // file, and the second silently replaced the first — which is the
        // failure mode that costs a player a republic.
        var file = $"{Sanitise(name)}-{date.Year:0000}{date.Month:00}{date.Day:00}"
            + $"-{Republic.Clock.Ticks % SimClock.TicksPerDay:0000}.rrsv";

        var why = Saves.Write(file, Republic);
        _said.Text = why == Error.Ok ? $"saved as {file}" : Trouble(why);

        Listing();
    }

    /// <summary>
    /// What went wrong with a file, in words.
    /// </summary>
    /// <remarks>
    /// The engine's own error names are not sentences and were being printed
    /// straight at the player: "could not save: FileCantWrite" is a symbol from
    /// somebody else's enum. The three that a player can actually do something
    /// about are named; the rest say so rather than pretending.
    /// </remarks>
    private static string Trouble(Error why) => why switch
    {
        Error.FileNoPermission => "the folder this game saves into cannot be written to",
        Error.FileCantOpen or Error.FileCantWrite =>
            "the file could not be written — the disk may be full",
        Error.FileNotFound => "that republic is no longer on file",
        Error.FileCorrupt or Error.InvalidData => "that file is not a republic this build can read",
        _ => "the file could not be written",
    };

    private void Listing()
    {
        Clear(_saves);
        Parts.Head(
            _saves,
            new Column("Republic", 2.0f, HorizontalAlignment.Left),
            new Column("Written", 1.2f, HorizontalAlignment.Right),
            new Column("", 1.0f, HorizontalAlignment.Right));

        var files = Saves.OnFile();
        for (var i = 0; i < files.Count; i++)
        {
            var file = files[i];
            var line = Parts.Row(_saves, i % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say(file.Name), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure(file.Written.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture)),
                1.2f, HorizontalAlignment.Right));

            var take = Parts.Press("Take it up", "Quiet");
            var path = file.File;
            take.Pressed += () =>
            {
                var republic = Saves.Read(path, Republic.Tables, out var why);
                if (republic is null)
                {
                    _said.Text = $"could not read {file.Name}: {why}";
                    return;
                }

                _said.Text = $"took up {file.Name}";
                Loaded?.Invoke(republic);
            };

            line.AddChild(Parts.Cell(take, 1.0f));
        }

        if (files.Count == 0)
        {
            _saves.AddChild(Parts.Prose("Nothing has been saved yet.", "Faint"));
        }
    }

    /// <summary>
    /// A republic's name, as something a filesystem will take.
    /// </summary>
    /// <remarks>
    /// The name a player typed is kept inside the save, so this only has to be
    /// unique enough to tell two files apart — it is never what the player is
    /// shown.
    /// </remarks>
    private static string Sanitise(string name)
    {
        var clean = new System.Text.StringBuilder();
        foreach (var c in name)
        {
            clean.Append(char.IsLetterOrDigit(c) ? c : '-');
        }

        return clean.ToString();
    }
}
