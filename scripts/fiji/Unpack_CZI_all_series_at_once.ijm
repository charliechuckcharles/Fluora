// Save every series from a CZI as a separate TIFF using Bio-Formats Importer

// 1) Ask user for the main CZI file
cziPath = File.openDialog("Choose CZI file");
if (cziPath == "") exit("No file selected");

// 2) Ask user for output folder
outDir = getDirectory("Choose output folder");
// if (outDir == null) exit("No output folder selected"); 

// Get base name of the CZI (without extension)
baseName = File.getNameWithoutExtension(cziPath);

// 3) Use Bio-Formats Importer to open ALL series
//    (no Ext.getSeriesCount needed)
run("Bio-Formats Importer", 
    "open=[" + cziPath + "] " +
    "autoscale " +
    "color_mode=Default " +
    "view=Hyperstack " +
    "stack_order=XYCZT " +
    "open_all_series");

// Number of open images (one per series)
n = nImages;
print("Number of open images (series): " + n);

// 4) Loop over all open images and save each as TIFF
// NOTE: We loop backwards (n down to 1) so closing windows doesn't mess up indices
for (i = n; i >= 1; i--) {
    selectImage(i);          // select the i-th image window
    title = getTitle();      // current window title (for debugging/info)

    // Build filename: baseName_seriesX.tif
    // Here we use (i-1) as a series index to match 0-based series numbering
    savePath = outDir + baseName + "_series" + (i-1) + ".tif";
    print("Saving " + title + " as " + savePath);

    saveAs("Tiff", savePath);
    close();
}

print("Done. Saved " + n + " TIFF files in:");
print(outDir);