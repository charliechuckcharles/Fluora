// Save every series from a CZI as a separate TIFF, one-at-a-time

// 1) Ask user for the main CZI file
cziPath = File.openDialog("Choose CZI file");

// 2) Ask user for output folder
outDir = getDirectory("Choose output folder");

// Get base name of the CZI (without extension)
baseName = File.getNameWithoutExtension(cziPath);

// 3) Enable Bio-Formats Macro Extensions
run("Bio-Formats Macro Extensions");

// 4) Tell Bio-Formats which file we are working with
Ext.setId(cziPath);

// 5) Get how many series are in this CZI
//    Your installation expects a single argument: the variable to fill
Ext.getSeriesCount(seriesCount);
print("Number of series in file: " + seriesCount);

// 6) Loop over each series (Bio-Formats Importer uses 1-based: series_1, series_2, ...)
for (s = 1; s <= seriesCount; s++) {

    // Open only this series with Bio-Formats Importer
    run("Bio-Formats Importer",
        "open=[" + cziPath + "] " +
        "autoscale " +
        "color_mode=Default " +
        "view=Hyperstack " +
        "stack_order=XYCZT " +
        "series_" + s);

    // Build filename. (s-1) keeps 0-based naming: series0, series1, ...
    savePath = outDir + baseName + "_series" + (s-1) + ".tif";
    print("Saving series " + (s-1) + " as " + savePath);

    // Save and close to free memory
    saveAs("Tiff", savePath);
    close();
    run("Collect Garbage");
}

print("Done. Saved " + seriesCount + " TIFF files in:");
print(outDir);